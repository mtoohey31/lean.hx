use abi_stable::std_types::{RHashMap, RSliceMut, Tuple2};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode, header},
    response::Html,
    routing::{delete, get, post},
};
use futures_util::SinkExt;
use include_dir::Dir;
use lsp_types::{InitializeResult, Location};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};
use std::{net::SocketAddr, sync::Arc};
use steel::{
    SteelErr, declare_module,
    rerrs::ErrorKind,
    rvals::{Custom, Result},
    steel_vm::ffi::{FFIArg, FFIModule, FFIValue, HostRuntimeFunction, RegisterFFIFn},
};
use tokio::{
    net::TcpListener,
    sync::{
        Mutex,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
        oneshot,
    },
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::protocol::Message;

const INDEX_HTML: &str = include_str!("../index.html");
const INDEX_MJS: &str = include_str!("../index.mjs");
const INFOVIEW_DIR: Dir = include_dir::include_dir!("$INFOVIEW_DIST");

fn value_from_ffi_arg(arg: FFIArg) -> Result<Value> {
    match arg {
        FFIArg::StringRef(rstr) => Ok(Value::String(rstr.to_string())),
        FFIArg::BoolV(b) => Ok(Value::Bool(b)),
        FFIArg::NumV(f) => Ok(Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)),
        FFIArg::IntV(i) => Ok(Value::Number(Number::from(i))),
        FFIArg::Void => Ok(Value::Null),
        FFIArg::StringV(rstring) => Ok(Value::String(rstring.to_string())),
        FFIArg::Vector(rvec) => Ok(Value::Array(
            rvec.into_iter()
                .map(value_from_ffi_arg)
                .collect::<Result<Vec<_>>>()?,
        )),
        FFIArg::CharV { c } => Ok(Value::String(c.to_string())),
        FFIArg::HashMap(rhash_map) => Ok(Value::Object(
            rhash_map
                .into_iter()
                .map(|Tuple2(k, v)| {
                    let k = match k {
                        FFIArg::StringRef(rstr) => rstr.to_string(),
                        FFIArg::StringV(rstring) => rstring.to_string(),
                        FFIArg::SymbolV(rstring) => rstring.to_string(),
                        _ => {
                            return Err(SteelErr::new(
                                ErrorKind::ConversionError,
                                "map keys must be strings or symbols for conversion to json value"
                                    .to_string(),
                            ));
                        }
                    };
                    Ok((k, value_from_ffi_arg(v)?))
                })
                .collect::<Result<Map<_, _>>>()?,
        )),
        FFIArg::ByteVector(rvec) => Ok(Value::Array(
            rvec.into_iter()
                .map(|b| Value::Number(Number::from(b)))
                .collect(),
        )),
        _ => Err(SteelErr::new(
            ErrorKind::ConversionError,
            "unsupported ffi argument variant for conversion to json value".to_string(),
        )),
    }
}

fn value_into_ffi_val(value: Value) -> Result<FFIValue> {
    match value {
        Value::Null => Ok(FFIValue::Void),
        Value::Bool(b) => Ok(FFIValue::BoolV(b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if let Ok(i) = TryInto::<isize>::try_into(i) {
                    return Ok(FFIValue::IntV(i));
                }
            }

            n.as_f64().map(FFIValue::NumV).ok_or(SteelErr::new(
                ErrorKind::ConversionError,
                "json number out of range for conversion to ffi value".to_string(),
            ))
        }
        Value::String(s) => Ok(FFIValue::StringV(s.into())),
        Value::Array(v) => Ok(FFIValue::Vector(
            v.into_iter()
                .map(value_into_ffi_val)
                .collect::<Result<Vec<_>>>()?
                .into(),
        )),
        Value::Object(m) => Ok(FFIValue::HashMap(
            m.into_iter()
                .map(|(k, v)| Ok(Tuple2(FFIValue::StringV(k.into()), value_into_ffi_val(v)?)))
                .collect::<Result<RHashMap<_, _>>>()?,
        )),
    }
}

struct UnboundedJsonSender(UnboundedSender<Value>);

impl Custom for UnboundedJsonSender {}

impl UnboundedJsonSender {
    fn send(&self, value: FFIArg) -> Result<()> {
        self.0
            .send(value_from_ffi_arg(value)?)
            .map_err(|e| SteelErr::new(ErrorKind::Generic, format!("{e}")))
    }
}

struct OneshotJsonSender(Option<oneshot::Sender<serde_json::Value>>);

impl Custom for OneshotJsonSender {}

impl OneshotJsonSender {
    fn send(&mut self, value: FFIArg) -> Result<()> {
        let Some(tx) = self.0.take() else {
            return Err(SteelErr::new(
                ErrorKind::Generic,
                "cannot send twice".to_string(),
            ));
        };

        tx.send(value_from_ffi_arg(value)?)
            .map_err(|e| SteelErr::new(ErrorKind::Generic, e.to_string()))
    }
}

pub struct ServerInner {
    loc: Location,
    initialize_result: InitializeResult,

    send_client_request: HostRuntimeFunction,
    send_client_notification: HostRuntimeFunction,
    subscribe_server_notifications: HostRuntimeFunction,
    unsubscribe_server_notifications: HostRuntimeFunction,
    create_rpc_session: HostRuntimeFunction,
    close_rpc_session: HostRuntimeFunction,

    ws_rx: UnboundedReceiver<serde_json::Value>,
}

pub struct Server(Option<ServerInner>);

impl Custom for Server {}

impl Server {
    pub fn new(
        loc: FFIArg,
        initialize_result: FFIArg,
        send_client_request: HostRuntimeFunction,
        send_client_notification: HostRuntimeFunction,
        subscribe_server_notifications: HostRuntimeFunction,
        unsubscribe_server_notifications: HostRuntimeFunction,
        create_rpc_session: HostRuntimeFunction,
        close_rpc_session: HostRuntimeFunction,
    ) -> Result<Vec<FFIValue>> {
        let (ws_tx, ws_rx) = unbounded_channel();

        let loc: Location = serde_json::from_value(value_from_ffi_arg(loc)?)
            .map_err(|e| SteelErr::new(ErrorKind::Parse, format!("{e}")))?;
        let initialize_result: InitializeResult =
            serde_json::from_value(value_from_ffi_arg(initialize_result)?)
                .map_err(|e| SteelErr::new(ErrorKind::Parse, format!("{e}")))?;

        Ok(vec![
            Self(Some(ServerInner {
                loc,
                initialize_result,
                send_client_request,
                send_client_notification,
                subscribe_server_notifications,
                unsubscribe_server_notifications,
                create_rpc_session,
                close_rpc_session,
                ws_rx,
            }))
            .into(),
            UnboundedJsonSender(ws_tx).into(),
        ])
    }

    pub fn listen(&mut self) -> Result<()> {
        let Some(ServerInner {
            loc,
            initialize_result,
            send_client_request,
            send_client_notification,
            subscribe_server_notifications,
            unsubscribe_server_notifications,
            create_rpc_session,
            close_rpc_session,
            ws_rx,
        }) = self.0.take()
        else {
            return Err(SteelErr::new(
                ErrorKind::Generic,
                "server cannot be listened to more than once".to_string(),
            ));
        };

        let send_client_request = Arc::new(send_client_request);
        let send_client_notification = Arc::new(send_client_notification);
        let subscribe_server_notifications = Arc::new(subscribe_server_notifications);
        let unsubscribe_server_notifications = Arc::new(unsubscribe_server_notifications);
        let create_rpc_session = Arc::new(create_rpc_session);
        let close_rpc_session = Arc::new(close_rpc_session);

        let (ws_rx_tx, ws_rx_rx) = oneshot::channel::<UnboundedReceiver<Value>>();
        let ws_rx = Arc::new(Mutex::new(Some((ws_rx, ws_rx_tx))));

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| SteelErr::new(ErrorKind::Io, format!("{e}")))?
            .block_on(Box::pin(async move {
                let sock_listener = TcpListener::bind("127.0.0.1:0").await?;
                let sock_addr: SocketAddr = sock_listener.local_addr()?;

                let sock_handle: JoinHandle<Result<_>> = tokio::spawn(Box::pin(async move {
                    let mut ws_stream =
                        tokio_tungstenite::accept_async(sock_listener.accept().await?.0)
                            .await
                            .map_err(|e| SteelErr::new(ErrorKind::Generic, format!("{e}")))?;

                    let mut ws_rx = ws_rx_rx
                        .await
                        .map_err(|e| SteelErr::new(ErrorKind::Generic, format!("{e}")))?;

                    while let Some(value) = ws_rx.recv().await {
                        let text = serde_json::to_string(&value)
                            .map_err(|e| SteelErr::new(ErrorKind::Generic, format!("{e}")))?
                            .into();

                        ws_stream
                            .send(Message::Text(text))
                            .await
                            .map_err(|e| SteelErr::new(ErrorKind::Generic, format!("{e}")))?;
                    }

                    Ok(())
                }));

                #[derive(Serialize)]
                struct InitializeResult {
                    loc: Location,
                    initialize_result: lsp_types::InitializeResult,
                }

                #[derive(Deserialize)]
                struct SendClientParams {
                    uri: String,
                    method: String,
                }

                #[derive(Deserialize)]
                struct ServerNotificationParams {
                    method: String,
                }

                #[derive(Deserialize)]
                struct CreateRpcParams {
                    uri: String,
                }

                #[derive(Deserialize)]
                struct CloseRpcParams {
                    session_id: String,
                }

                let app = Router::new()
                    .route("/", get(Html(INDEX_HTML)))
                    .route(
                        "/index.mjs",
                        get(([(header::CONTENT_TYPE, "text/javascript")], INDEX_MJS)),
                    )
                    .route(
                        "/infoview/{*path}",
                        get(async |Path(path): Path<String>| {
                            let Some(file) = INFOVIEW_DIR.get_file(&path) else {
                                return Err(StatusCode::NOT_FOUND);
                            };

                            let mut hm = HeaderMap::new();
                            if let Some(mime) = mime_guess::from_path(&path).first() {
                                hm.insert(
                                    header::CONTENT_TYPE,
                                    mime.to_string()
                                        .try_into()
                                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
                                );
                            }
                            Ok::<_, StatusCode>((hm, file.contents()))
                        }),
                    )
                    .route("/wsport", get(u16::to_string(&sock_addr.port())))
                    .route(
                        "/initialize",
                        get(async move || {
                            let Some((mut ws_rx, ws_rx_tx)) = ws_rx.lock().await.take() else {
                                return Err(StatusCode::TOO_MANY_REQUESTS);
                            };

                            let mut most_recent_msg = None;
                            while let Ok(loc) = ws_rx.try_recv() {
                                most_recent_msg = Some(loc);
                            }
                            ws_rx_tx
                                .send(ws_rx)
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                            let loc = if let Some(msg) = most_recent_msg {
                                #[derive(Deserialize)]
                                struct LocationMessage {
                                    loc: Location,
                                }

                                serde_json::from_value::<LocationMessage>(msg)
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                                    .loc
                            } else {
                                loc
                            };

                            Ok::<Json<_>, _>(
                                InitializeResult {
                                    loc,
                                    initialize_result,
                                }
                                .into(),
                            )
                        }),
                    )
                    .route(
                        "/sendclientrequest",
                        post(
                            async move |Query(SendClientParams { uri, method }),
                                        Json(params): Json<Value>| {
                                let (tx, rx) = oneshot::channel::<Value>();

                                send_client_request
                                    .call(RSliceMut::from_mut_slice(&mut [
                                        uri.into(),
                                        method.into(),
                                        value_into_ffi_val(params)
                                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
                                        OneshotJsonSender(Some(tx)).into(),
                                    ]))
                                    .into_result()
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                                Ok::<Json<_>, StatusCode>(
                                    rx.await
                                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                                        .into(),
                                )
                            },
                        ),
                    )
                    .route(
                        "/sendclientnotification",
                        post(
                            async move |Query(SendClientParams { uri, method }),
                                        Json(params): Json<Value>| {
                                send_client_notification
                                    .call(RSliceMut::from_mut_slice(&mut [
                                        uri.into(),
                                        method.into(),
                                        value_into_ffi_val(params)
                                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
                                    ]))
                                    .into_result()
                                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                                Ok::<_, StatusCode>(())
                            },
                        ),
                    )
                    .route(
                        "/subscribeservernotifications",
                        post(async move |Query(ServerNotificationParams { method })| {
                            subscribe_server_notifications
                                .call(RSliceMut::from_mut_slice(&mut [method.into()]))
                                .into_result()
                                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                            Ok::<_, (StatusCode, String)>(())
                        }),
                    )
                    .route(
                        "/unsubscribeservernotifications",
                        delete(async move |Query(ServerNotificationParams { method })| {
                            unsubscribe_server_notifications
                                .call(RSliceMut::from_mut_slice(&mut [method.into()]))
                                .into_result()
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                            Ok::<_, StatusCode>(())
                        }),
                    )
                    .route(
                        "/createrpcsession",
                        post(async move |Query(CreateRpcParams { uri })| {
                            let (tx, rx) = oneshot::channel::<Value>();

                            create_rpc_session
                                .call(RSliceMut::from_mut_slice(&mut [
                                    uri.into(),
                                    OneshotJsonSender(Some(tx)).into(),
                                ]))
                                .into_result()
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                            rx.await
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                                .as_str()
                                .map(ToString::to_string)
                                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
                        }),
                    )
                    .route(
                        "/closerpcsession",
                        delete(async move |Query(CloseRpcParams { session_id })| {
                            close_rpc_session
                                .call(RSliceMut::from_mut_slice(&mut [session_id.into()]))
                                .into_result()
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                            Ok::<_, StatusCode>(())
                        }),
                    );

                let serve_listener = TcpListener::bind("127.0.0.1:0").await?;
                let serve_addr: SocketAddr = serve_listener.local_addr()?;

                let serve_handle: JoinHandle<Result<()>> = tokio::spawn(Box::pin(async {
                    axum::serve(serve_listener, app.into_make_service()).await?;

                    Ok(())
                }));

                open::that(format!("http://{serve_addr}"))?;

                let res = tokio::select! {
                    res = sock_handle => res,
                    res = serve_handle => res,
                };
                res.map_err(|e| SteelErr::new(ErrorKind::Io, format!("{e}")))?
            }))
    }
}

declare_module!(create_module);

fn create_module() -> FFIModule {
    let mut module = FFIModule::new("lean.hx");

    module
        .register_fn("server", Server::new)
        .register_fn("server-listen!", Server::listen)
        .register_fn("unbounded-send", UnboundedJsonSender::send)
        .register_fn("oneshot-send", OneshotJsonSender::send);

    module
}
