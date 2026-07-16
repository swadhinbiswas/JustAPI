"""Integration test for the native Rust scheduler (cron + interval jobs).

Jobs are Python callables dispatched onto the same Rust background-task worker
pool as `BackgroundTasks`. v1 is UTC + in-memory (see ADR-060).
"""

import time

from justapi import JustAPIApp, Scheduler


def test_scheduler_interval_and_cron_fire():
    app = JustAPIApp()
    fired = []

    @app.get("/")
    def root():
        return {"ok": True}

    # interval job: every 1s
    app.every(1, lambda: fired.append("every"))

    # cron job (6-field: sec min hour dom mon dow), UTC: every 2 seconds
    Scheduler().schedule("*/2 * * * * *", lambda: fired.append("cron"))

    # invalid expression must raise at registration time
    try:
        Scheduler().schedule("not a cron", lambda: None)
        assert False, "invalid cron expression did not raise"
    except ValueError:
        pass

    sched = Scheduler()
    sched.start()
    try:
        time.sleep(3.6)
    finally:
        sched.stop()

    assert fired.count("every") >= 2, fired
    assert fired.count("cron") >= 1, fired
    # stats reflects total fires
    assert dict(sched.stats())["fired"] >= 3


def test_scheduler_remove_and_jobs():
    sched = Scheduler()
    jid = sched.every(60, lambda: None)
    jobs = sched.jobs()
    assert any(j["id"] == jid for j in jobs)
    assert sched.remove(jid) is True
    assert sched.remove(jid) is False  # already removed
