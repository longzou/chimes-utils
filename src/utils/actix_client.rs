extern crate tls_rustls as rustls;

use actix_web::{
    http::header::{self, HeaderMap, HeaderName},
    http::Method,
    web::Bytes,
};
use awc::{
    error::HeaderValue,
    ws::{self, Message},
    Client as HttpClient, Connector,
};

use openssl::ssl::{SslConnector, SslFiletype, SslMethod, SslVerifyMode};
use serde::Serialize;

use crate::{error, ChimesError, ChimesResult as Result};
use actix_tls::connect::rustls::webpki_roots_cert_store;
use encoding_rs::{Encoding, UTF_8};
use mime::Mime;
use rustls::ClientConfig;
use std::borrow::Cow;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

// use rustls_pemfile::{certs, pkcs8_private_keys};

pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/70.0.3534.4 Safari/537.36";

mod danger {
    // use awc::*;
    extern crate tls_rustls as rustls;
    use rustls::{
        client::{ServerCertVerified, ServerCertVerifier},
        Certificate, ServerName,
    };
    use std::time::SystemTime;
    pub struct NoCertificateVerification;

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &Certificate,
            _intermediates: &[Certificate],
            _server_name: &ServerName,
            _scts: &mut dyn Iterator<Item = &[u8]>,
            _ocsp_response: &[u8],
            _now: SystemTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }
    }
}

/// 请求客户端
#[derive(Clone)]
pub struct ChimesClient {
    pub(crate) client: HttpClient,
    charset: String,
    headers: header::HeaderMap,
}

// pub trait ToResult {
//     fn result(&self) -> Result<ResponseResult>;
// }

// impl tube_error::Result<String, tube_error::Error> for ToResult
// {
//      fn result(&self) -> Result<ResponseResult> {
//         match serde_json::from_str(self) {
//             Ok(rs) => Ok(rs),
//             Err(err) => Err(error! {
//                 code: -1,
//                 msg: format!("error: {}", err)
//             }),
//         }
//     }
// }

/// 将结果转换为字符串
pub(crate) fn text_with_charset(
    headers: &header::HeaderMap,
    default_encoding: &str,
    bs: Bytes,
) -> Result<String> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok());
    let encoding_name = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
        .unwrap_or(default_encoding);
    let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8);

    let (text, _, _) = encoding.decode(&bs);
    if let Cow::Owned(s) = text {
        return Ok(s);
    }
    unsafe {
        // decoding returned Cow::Borrowed, meaning these bytes
        // are already valid utf8
        Ok(String::from_utf8_unchecked(bs.to_vec()))
    }
}

impl Default for ChimesClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ChimesClient {
    pub fn new() -> Self {
        Self::new_timeout(60u64)
    }
    /// 创建一个新的连接客户端
    ///
    pub fn new_timeout(tm: u64) -> Self {
        // disable ssl verification
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);
        let _ = builder
            .set_alpn_protos(b"\x02h2\x08http/1.1")
            .map_err(|e| log::info!("Can not set alpn protocol: {:?}", e));

        let connector = Connector::new()
            .timeout(Duration::from_secs(tm))
            .handshake_timeout(Duration::from_secs(30))
            .openssl(builder.build());

        let client = HttpClient::builder()
            .connector(connector)
            .timeout(Duration::from_secs(tm))
            .add_default_header((header::USER_AGENT, DEFAULT_USER_AGENT))
            // .header(header::AUTHORIZATION, token)
            // .header(header::REFERER, "http://localhost")
            // .initial_window_size(100)
            // .initial_connection_window_size(100)
            .finish();

        ChimesClient {
            client,
            charset: "utf-8".to_owned(),
            headers: header::HeaderMap::new(),
        }
    }

    /**
     * 使用rust_tls进行HTTPS请求的客户端
     */
    pub fn new_tls() -> Self {
        // disable ssl verification
        let mut config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(webpki_roots_cert_store())
            .with_no_client_auth();

        let protos = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        config.alpn_protocols = protos;

        // disable TLS verification
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(danger::NoCertificateVerification));

        let client = awc::Client::builder()
            .connector(
                awc::Connector::new()
                    .rustls(Arc::new(config))
                    .handshake_timeout(Duration::from_secs(15))
                    .timeout(Duration::from_secs(15)),
            )
            .add_default_header((header::USER_AGENT, DEFAULT_USER_AGENT))
            .timeout(Duration::from_secs(30))
            .finish();

        ChimesClient {
            client,
            charset: "utf-8".to_owned(),
            headers: header::HeaderMap::new(),
        }
    }

    /// 带证书的远程请求客户端
    pub fn new_ssl(private_key: &str, certificate: &str) -> Self {
        // disable ssl verification
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);
        let _ = builder
            .set_alpn_protos(b"\x02h2\x08http/1.1")
            .map_err(|e| log::info!("Can not set alpn protocol: {:?}", e));

        let _ = builder
            .set_private_key_file(private_key, SslFiletype::PEM)
            .map_err(|e| log::info!("apiclient_key.pem not find: {:?}", e));
        let _ = builder
            .set_certificate_chain_file(certificate)
            .map_err(|e| log::info!("apiclient_cert.pem not find: {:?}", e));

        let connector = Connector::new()
            .timeout(Duration::from_secs(5))
            .openssl(builder.build());

        let client = HttpClient::builder()
            .connector(connector)
            .add_default_header((header::USER_AGENT, DEFAULT_USER_AGENT))
            .finish();

        ChimesClient {
            client,
            charset: "utf-8".to_owned(),
            headers: header::HeaderMap::new(),
        }
    }

    /// 设置获取数据的编码方式
    pub fn set_charset(mut self, charset: &str) -> Self {
        self.charset = charset.to_owned();
        self
    }

    /// get方式获取站点内容
    pub async fn get(self, url: &str) -> Result<String> {
        let mut build = self.client.get(url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send().await {
            Ok(mut res) => {
                // log!("{:?}", res);
                if res.status().is_success() {
                    // match res.json() {
                    //     Ok(res) => {},
                    //     Err(err) => {}
                    // }
                    match res.body().await {
                        Ok(bs) => {
                            let s = text_with_charset(res.headers(), &self.charset, bs);
                            // println!("{:?}", s);
                            s
                        }
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    Err(error! {
                        code: 500,
                        msg: format!("status={}", res.status())
                    })
                }
            }
            Err(e) => {
                log::info!("=== request error === {:?}", e);
                Err(error! {
                    code: 500,
                    msg: format!("Send request error: {}", e)
                })
            }
        }
    }
    /// 返回bytes
    pub async fn get_bytes(self, url: &str) -> Result<Vec<u8>> {
        let mut build = self.client.get(url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send().await {
            Ok(mut res) => {
                if res.status().is_success() {
                    match res.body().await {
                        Ok(bs) => Ok(bs[..].to_vec()),
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    Err(error! {
                        code: 500,
                        msg: format!("status={}", res.status())
                    })
                }
            }
            Err(e) => {
                log::info!("=== request error === {:?}", e);
                Err(error! {
                    code: 500,
                    msg: format!("Send request error: {}", e)
                })
            }
        }
    }
    /// post方式提交数据
    /// url:
    /// param:
    pub async fn post<T: Serialize>(self, url: &str, params: &T) -> Result<String> {
        self.request(Method::POST, url, params).await
    }

    /// 请求put方式
    pub async fn put<T: Serialize>(self, url: &str, params: &T) -> Result<String> {
        self.request(Method::PUT, url, params).await
    }

    /// 请求删除方式
    pub async fn delete<T: Serialize>(self, url: &str, params: &T) -> Result<String> {
        self.request(Method::DELETE, url, params).await
    }

    // /// 解析并转换为tube_value::Value
    // pub fn parse_value(&self, text: &str) -> Result<Value> {

    //     let val: serde_json::Value = match serde_json::from_str(post_str) {
    //         Ok(val) => val,
    //         Err(err) => Err(error! {
    //             code: -1,
    //             msg: format!("error: {}", err)
    //         }),
    //     };

    //     Ok(val.to_value())

    // }

    /// 请求
    pub async fn request_betyes<T: Serialize>(
        self,
        method_str: &str,
        url: &str,
        params: &T,
    ) -> Result<Vec<u8>> {
        // log!("params === {:?}", params);
        let method = match Method::from_bytes(method_str.as_bytes()) {
            Ok(s) => s,
            Err(_e) => Method::POST,
        };
        let mut build = self.client.request(method, url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send_json(params).await {
            Ok(mut res) => {
                // log!("response: {:?}", res);
                if res.status().is_success() {
                    match res.body().await {
                        Ok(bs) => Ok(bs.to_vec()),
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    Err(error! {
                        code: 500,
                        msg: format!("status={}", res.status())
                    })
                }
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e)
            }),
        }
    }

    /// 请求
    pub async fn request<T: Serialize>(
        self,
        method: Method,
        url: &str,
        params: &T,
    ) -> Result<String> {
        let mut build = self.client.request(method, url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send_json(params).await {
            Ok(mut res) => {
                // log!("response: {:?}", res);
                if res.status().is_success() {
                    match res.body().await {
                        Ok(bs) => match text_with_charset(res.headers(), &self.charset, bs) {
                            Ok(s) => {
                                // println!(" === {:?}", s);
                                // match serde_json::from_str(&s) {
                                //     Ok(rs) => Ok(rs),
                                //     Err(err) => Err(error! {
                                //         code: -1,
                                //         msg: format!("error: {}", err)
                                //     }),
                                // }
                                Ok(s)
                            }
                            Err(err) => Err(err),
                        },
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    Err(error! {
                        code: 500,
                        msg: format!("status={}", res.status())
                    })
                }
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e)
            }),
        }
    }

    /// 请求
    pub async fn request_form_with_response(
        self,
        method: Method,
        url: &str,
        params: &String,
    ) -> Result<(String, HeaderMap)> {
        let mut build = self.client.request(method, url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send_body(params.to_owned()).await {
            Ok(mut res) => {
                // log!("response: {:?}", res);
                if res.status().is_success() {
                    match res.body().await {
                        Ok(bs) => match text_with_charset(res.headers(), &self.charset, bs) {
                            Ok(s) => {
                                // println!(" === {:?}", s);
                                // match serde_json::from_str(&s) {
                                //     Ok(rs) => Ok(rs),
                                //     Err(err) => Err(error! {
                                //         code: -1,
                                //         msg: format!("error: {}", err)
                                //     }),
                                // }
                                Ok((s, res.headers().to_owned()))
                            }
                            Err(err) => Err(err),
                        },
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    let resp = match res.body().await {
                        Ok(bs) => match text_with_charset(res.headers(), &self.charset, bs) {
                            Ok(s) => s,
                            Err(err) => err.to_string(),
                        },
                        Err(err) => err.to_string(),
                    };
                    log::info!("Stop {}", resp.clone());
                    Err(error! {
                        code: 500,
                        msg: format!("status={}, message={}", res.status(), resp)
                    })
                }
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e)
            }),
        }
    }

    /// 请求
    pub async fn request_with_response<T: Serialize>(
        self,
        method: Method,
        url: &str,
        params: &T,
    ) -> Result<(String, HeaderMap)> {
        let mut build = self.client.request(method, url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send_json(params).await {
            Ok(mut res) => {
                // log!("response: {:?}", res);
                if res.status().is_success() {
                    match res.body().await {
                        Ok(bs) => match text_with_charset(res.headers(), &self.charset, bs) {
                            Ok(s) => {
                                // println!(" === {:?}", s);
                                // match serde_json::from_str(&s) {
                                //     Ok(rs) => Ok(rs),
                                //     Err(err) => Err(error! {
                                //         code: -1,
                                //         msg: format!("error: {}", err)
                                //     }),
                                // }
                                Ok((s, res.headers().to_owned()))
                            }
                            Err(err) => Err(err),
                        },
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    let resp = match res.body().await {
                        Ok(bs) => match text_with_charset(res.headers(), &self.charset, bs) {
                            Ok(s) => s,
                            Err(err) => err.to_string(),
                        },
                        Err(err) => err.to_string(),
                    };
                    log::info!("Stop {}", resp.clone());
                    Err(error! {
                        code: 500,
                        msg: format!("status={}, message={}", res.status(), resp)
                    })
                }
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e)
            }),
        }
    }

    pub fn insert_header(&mut self, key: &str, value: &str) -> &mut Self {
        let hn = match HeaderName::from_lowercase(key.to_lowercase().as_bytes()) {
            Ok(v) => Some(v),
            Err(_) => None,
        };

        let hv = match HeaderValue::from_str(value) {
            Ok(v) => Some(v),
            Err(_) => None,
        };

        if let Some(hn) = hn {
            if let Some(hv) = hv {
                self.headers.insert(hn, hv);
            }
        }

        self
    }

    pub fn put_header(&mut self, key: &str, value: &str) {
        let hn = match HeaderName::from_lowercase(key.to_lowercase().as_bytes()) {
            Ok(v) => Some(v),
            Err(_) => None,
        };

        let hv = match HeaderValue::from_str(value) {
            Ok(v) => Some(v),
            Err(_) => None,
        };

        if let Some(hn) = hn {
            if let Some(hv) = hv {
                self.headers.insert(hn, hv);
            }
        }
    }
    /// 发送二进制文件
    pub async fn post_betyes(self, url: &str, body: Bytes) -> Result<String> {
        let mut build = self.client.post(url);
        for (head_name, head_value) in self.headers {
            build = build.insert_header((head_name, head_value));
        }
        match build.send_body(body).await {
            Ok(mut res) => {
                if res.status().is_success() {
                    match res.body().await {
                        Ok(bs) => text_with_charset(res.headers(), &self.charset, bs),
                        Err(err) => Err(error! {
                            code: -1,
                            msg: format!("error: {}", err)
                        }),
                    }
                } else {
                    Err(error! {
                        code: 500,
                        msg: format!("status={}", res.status())
                    })
                }
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e)
            }),
        }
    }
}

use futures_util::{SinkExt as _, StreamExt as _};
use std::sync::Condvar;

pub struct ChimesWebSocketClient {
    websocket_url: String,
    send_queue: Vec<String>,
    recv_queue: Vec<String>,
    looping: bool,
    mutex_send: Mutex<bool>,
    cond_send: Condvar,
}

impl ChimesWebSocketClient {
    pub fn new_websocket(url: &str) -> Result<Self> {
        Ok(Self {
            websocket_url: url.to_string(),
            send_queue: vec![],
            recv_queue: vec![],
            looping: false,
            mutex_send: Mutex::new(false),
            cond_send: Condvar::new(),
        })
    }

    pub fn send(&mut self, body: &str) -> Result<()> {
        self.send_queue.push(body.to_string());
        Ok(())
    }

    pub fn recv(&self) -> &Vec<String> {
        &self.recv_queue
    }

    pub fn stop(mut self) {
        self.looping = false;
        let lock = self.mutex_send.lock().unwrap();
        self.cond_send.notify_all();
        drop(lock);
    }

    pub async fn start(&mut self) -> Result<()> {
        self.looping = true;
        while self.looping {
            let cli = awc::Client::new().ws(self.websocket_url.as_str());
            match cli.connect().await {
                Ok((_cl, mut conn)) => {
                    let resp = conn.next().await;
                    if resp.is_none() {
                        // Received None, it means the connection was lost.
                        let lock = self.mutex_send.lock().expect("Can not lock");
                        match self.cond_send.wait_timeout(lock, Duration::from_secs(1)) {
                            Ok((_l, result)) => {
                                if result.timed_out() {
                                    break;
                                }
                            }
                            Err(err) => {
                                log::info!(
                                    "Condvar cond_send wait for a timeout with error. {}",
                                    err
                                );
                            }
                        }

                        let mut top_msg = self.send_queue.pop();
                        while top_msg.is_some() {
                            let msg = top_msg.unwrap();
                            match conn.send(ws::Message::Text(msg.clone().into())).await {
                                Ok(_) => {}
                                Err(err) => {
                                    log::info!("Send msg with error {}", err);
                                    self.send_queue.insert(0, msg);
                                    break;
                                }
                            }
                            top_msg = self.send_queue.pop();
                        }

                        // if self.send_queue.is_empty() {}
                        break;
                    } else {
                        match resp.unwrap() {
                            Ok(frame) => {
                                match frame {
                                    ws::Frame::Text(text) => self
                                        .recv_queue
                                        .push(String::from_utf8(text.to_vec()).unwrap()),
                                    ws::Frame::Binary(bin) => self
                                        .recv_queue
                                        .push(String::from_utf8(bin.to_vec()).unwrap()),
                                    ws::Frame::Continuation(_t) => {}
                                    ws::Frame::Ping(ping) => {
                                        let msg = String::from_utf8(ping.to_vec()).unwrap();
                                        log::info!("recv ping message {}", msg);
                                        let _ =
                                            conn.send(Message::Pong("pong".into())).await.is_ok();
                                    }
                                    ws::Frame::Pong(pong) => {
                                        let msg = String::from_utf8(pong.to_vec()).unwrap();
                                        log::info!("recv pong message {}", msg);
                                        let _ =
                                            conn.send(Message::Ping("ping".into())).await.is_ok();
                                    }
                                    ws::Frame::Close(_close) => {
                                        // should close
                                        break;
                                    }
                                }
                            }
                            Err(err) => {
                                log::info!("Unsupport protocol error {}", err);
                            }
                        };
                    }
                }
                Err(err) => {
                    log::info!("WebSocket error {}", err);
                }
            }
            let lock = self.mutex_send.lock().unwrap();
            let (l, _r) = self
                .cond_send
                .wait_timeout(lock, Duration::from_secs(1))
                .unwrap();
            drop(l);
        }
        Ok(())
    }
}
