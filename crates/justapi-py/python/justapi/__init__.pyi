from . import testing as testing, tracing as tracing, status as status, auth as auth, logging as logging
from typing import Any, Callable, Optional
from ._justapi import Dag as Dag, DagNode as DagNode, Database as Database, DbParam as DbParam, DbPool as DbPool, HTTPConnection as HTTPConnection, Headers as Headers, QueryParams as QueryParams, RateLimitResult as RateLimitResult, RateLimiter as RateLimiter, Request as Request, RequestStream as RequestStream, Scheduler as Scheduler, State as State, TokenStreamResponse as TokenStreamResponse, URL as URL, UploadFile as UploadFile, ValidatedStreamResponse as ValidatedStreamResponse, serve as serve, init_logging as init_logging, init_json_logging as init_json_logging, init_file_logging as init_file_logging, init_otlp_tracing as init_otlp_tracing, shutdown_tracing as shutdown_tracing
from .app import APIRouter as APIRouter, Controller as Controller, JustAPIApp as JustAPIApp, JustAPI as JustAPI, JustAPP as JustAPP, JustAPITestClient as JustAPITestClient, Mailer as Mailer, RequestValidationError as RequestValidationError, Session as Session, adaptive_batch as adaptive_batch, controller as controller, native_async as native_async, route_delete as route_delete, route_get as route_get, route_post as route_post, route_put as route_put, route_patch as route_patch, route_query as route_query, route_sse as route_sse, route_websocket as route_websocket
from .auth import APIKeyCookie as APIKeyCookie, APIKeyHeader as APIKeyHeader, APIKeyQuery as APIKeyQuery, HTTPBasic as HTTPBasic, HTTPBearer as HTTPBearer, JwtAuth as JwtAuth, OAuth2PasswordBearer as OAuth2PasswordBearer, OAuth2PasswordRequestForm as OAuth2PasswordRequestForm, OAuth2PasswordRequestFormStrict as OAuth2PasswordRequestFormStrict
from .background import BackgroundTasks as BackgroundTasks
from .exceptions import HTTPException as HTTPException, WebSocketException as WebSocketException
from .params import Body as Body, Cookie as Cookie, File as File, Form as Form, Header as Header, Param as Param, Path as Path, Query as Query
from .responses import FileResponse as FileResponse, HTMLResponse as HTMLResponse, JSONResponse as JSONResponse, PlainTextResponse as PlainTextResponse, RedirectResponse as RedirectResponse, Response as Response, StreamingResponse as StreamingResponse
from .system import build_help as build_help, build_openapi as build_openapi, register_system_routes as register_system_routes
from .templating import Jinja2Templates as Jinja2Templates
from .websockets import WebSocket as WebSocket, WebSocketDisconnect as WebSocketDisconnect, WebSocketState as WebSocketState

# FastAPI-style factories (runtime: classes in justapi.app). Declared as
# functions returning Any so the DI default idiom type-checks:
#   def handler(token: str = Depends(get_token)): ...
def Depends(dependency: Callable[..., Any], use_cache: bool = True) -> Any: ...
def Security(dependency: Callable[..., Any], use_cache: bool = True, scopes: Optional[list[str]] = None) -> Any: ...

__all__ = ['__version__', 'serve', 'JustAPIApp', 'JustAPI', 'JustAPP', 'Depends', 'Security', 'Mailer', 'Database', 'DbPool', 'DbParam', 'Schema', 'pydantic_schema', 'JustAPITestClient', 'testing', 'tracing', 'auth', 'Jinja2Templates', 'BackgroundTasks', 'Scheduler', 'TokenStreamResponse', 'ValidatedStreamResponse', 'WebSocket', 'Dag', 'DagNode', 'RateLimiter', 'RateLimitResult', 'adaptive_batch', 'native_async', 'APIRouter', 'Controller', 'controller', 'route_get', 'route_post', 'route_put', 'route_patch', 'route_delete', 'route_query', 'route_sse', 'route_websocket', 'Request', 'HTTPConnection', 'Headers', 'QueryParams', 'URL', 'State', 'Session', 'RequestStream', 'Response', 'HTMLResponse', 'PlainTextResponse', 'JSONResponse', 'RedirectResponse', 'StreamingResponse', 'FileResponse', 'UploadFile', 'RequestValidationError', 'Param', 'Path', 'Query', 'Header', 'Cookie', 'Body', 'File', 'Form', 'status', 'HTTPException', 'WebSocketException', 'WebSocketState', 'WebSocketDisconnect', 'JwtAuth', 'OAuth2PasswordBearer', 'OAuth2PasswordRequestForm', 'OAuth2PasswordRequestFormStrict', 'HTTPBearer', 'HTTPBasic', 'APIKeyHeader', 'APIKeyQuery', 'APIKeyCookie', 'build_help', 'build_openapi', 'register_system_routes']

class Schema:
    def __init_subclass__(cls, **kwargs) -> None: ...

def pydantic_schema(model_class) -> str: ...
