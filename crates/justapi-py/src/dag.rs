use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[pyclass(name = "DagNode", from_py_object)]
pub struct DagNode {
    pub name: String,
    pub handler: Py<PyAny>,
    pub dependencies: Vec<String>,
}

impl Clone for DagNode {
    fn clone(&self) -> Self {
        Python::<'_>::try_attach(|py| Self {
            name: self.name.clone(),
            handler: self.handler.clone_ref(py),
            dependencies: self.dependencies.clone(),
        })
        .expect("GIL required for Clone")
    }
}

#[pymethods]
impl DagNode {
    #[new]
    #[pyo3(signature = (name, handler, dependencies=None))]
    fn new(name: String, handler: Py<PyAny>, dependencies: Option<Vec<String>>) -> Self {
        Self { name, handler, dependencies: dependencies.unwrap_or_default() }
    }
}

#[pyclass(name = "Dag", from_py_object)]
#[derive(Clone)]
pub struct Dag {
    nodes: HashMap<String, DagNode>,
}

#[pymethods]
impl Dag {
    #[new]
    fn new(nodes: Vec<DagNode>) -> PyResult<Self> {
        let mut map = HashMap::new();
        for node in nodes {
            map.insert(node.name.clone(), node);
        }
        Ok(Self { nodes: map })
    }

    /// This function returns an `asyncio.Future` in Python.
    fn execute<'py>(
        &self,
        py: Python<'py>,
        inputs: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let nodes = self.nodes.clone();

        let mut state: HashMap<String, Py<PyAny>> = HashMap::new();
        for (k, v) in inputs.iter() {
            if let Ok(key_str) = k.extract::<String>() {
                state.insert(key_str, v.into());
            }
        }

        let state = Arc::new(tokio::sync::RwLock::new(state));
        let notify = Arc::new(Notify::new());
        let cancel = CancellationToken::new();

        // 1. Get asyncio loop and create a future
        let asyncio = py.import("asyncio")?;
        let loop_obj = asyncio.call_method0("get_event_loop")?;
        let future_obj = loop_obj.call_method0("create_future")?;

        let loop_py = loop_obj.into_any().unbind();
        let fut_py = future_obj.clone().unbind();

        // 2. Decide how to run it based on whether we are in a tokio context
        let handle = tokio::runtime::Handle::try_current();
        match handle {
            Ok(rt) => {
                rt.spawn(run_dag_internal(nodes, state, notify, cancel, loop_py, fut_py));
            }
            Err(_) => {
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(run_dag_internal(nodes, state, notify, cancel, loop_py, fut_py));
                });
            }
        }

        Ok(future_obj)
    }
}

async fn run_dag_internal(
    nodes: HashMap<String, DagNode>,
    state: Arc<tokio::sync::RwLock<HashMap<String, Py<PyAny>>>>,
    notify: Arc<Notify>,
    cancel: CancellationToken,
    loop_py: Py<PyAny>,
    fut_py: Py<PyAny>,
) {
    let mut pending: HashSet<String> = nodes.keys().cloned().collect();
    let mut completed: HashSet<String> = HashSet::new();
    let mut running: HashSet<String> = HashSet::new();
    let mut tasks = tokio::task::JoinSet::new();

    let result = loop {
        if pending.is_empty() && running.is_empty() {
            break Ok(());
        }

        let state_guard = state.read().await;
        let mut ready = Vec::new();
        for node_name in &pending {
            let node = &nodes[node_name];
            let can_run = node.dependencies.iter().all(|d| state_guard.contains_key(d));
            if can_run {
                ready.push(node_name.clone());
            }
        }
        drop(state_guard);

        for r in ready {
            pending.remove(&r);
            running.insert(r.clone());

            let node = nodes[&r].clone();
            let state_clone = state.clone();
            let notify_clone = notify.clone();

            tasks.spawn_blocking(move || {
                let res = Python::attach(|py| -> Result<Py<PyAny>, String> {
                    let sg = state_clone.blocking_read();
                    let mut args = Vec::new();
                    for dep in &node.dependencies {
                        if let Some(val) = sg.get(dep) {
                            args.push(val.clone_ref(py));
                        }
                    }
                    drop(sg);

                    let py_tuple = PyTuple::new(py, args).expect("tuple");
                    match node.handler.call1(py, py_tuple) {
                        Ok(val) => Ok(val),
                        Err(e) => Err(e.to_string()),
                    }
                });
                notify_clone.notify_waiters();
                (r, res)
            });
        }

        if !running.is_empty() {
            tokio::select! {
                Some(res) = tasks.join_next() => {
                    match res {
                        Ok((name, result)) => {
                            running.remove(&name);
                            completed.insert(name.clone());
                            match result {
                                Ok(py_obj) => {
                                    state.write().await.insert(name, py_obj);
                                    notify.notify_waiters();
                                }
                                Err(e) => {
                                    tracing::error!("DAG node {} failed: {}", name, e);
                                    cancel.cancel();
                                    break Err(format!("Node {} failed: {}", name, e));
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("DAG task join error: {}", e);
                            cancel.cancel();
                            break Err(format!("Join error: {}", e));
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    break Err("DAG cancelled".to_string());
                }
            }
        } else if !pending.is_empty() {
            break Err(format!(
                "DAG deadlock: nodes {:?} are waiting for dependencies that never arrive",
                pending
            ));
        }
    };

    let final_state = state.read().await;
    // 3. Resolve the asyncio future in Python
    Python::attach(|py| {
        let call_soon_ts = loop_py.getattr(py, "call_soon_threadsafe").unwrap();
        match result {
            Ok(_) => {
                let dict = PyDict::new(py);
                for (k, v) in final_state.iter() {
                    dict.set_item(k, v).unwrap();
                }
                let set_result = fut_py.getattr(py, "set_result").unwrap();
                if let Err(e) = call_soon_ts.call1(py, (set_result, dict)) {
                    tracing::error!("Failed to call set_result: {:?}", e);
                    e.print(py);
                }
            }
            Err(e) => {
                let set_exception = fut_py.getattr(py, "set_exception").unwrap();
                let exceptions = py.import("builtins").unwrap();
                let runtime_error = exceptions.getattr("RuntimeError").unwrap();
                let err_obj = runtime_error.call1((e,)).unwrap();
                if let Err(e) = call_soon_ts.call1(py, (set_exception, err_obj)) {
                    tracing::error!("Failed to call set_exception: {:?}", e);
                    e.print(py);
                }
            }
        }
    });
}
