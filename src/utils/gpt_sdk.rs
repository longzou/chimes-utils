use futures_util::Future;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Mutex;

use crate::{
    bool_from_str, get_task_queue, i64_from_str, queue_registry_handler, AppConfig, ChimesClient,
    ChimesError, ChimesPerformanceInfo, ProcessTask,
};

lazy_static! {
    pub static ref MAP_GATEWAY_PROXY: Mutex<RefCell<HashMap<String, GatewayProxyManagement>>> =
        Mutex::new(RefCell::new(HashMap::new()));
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiResult<T> {
    pub status: i32,
    pub message: String,
    pub data: Option<T>,
    pub timestamp: Option<u64>,
}

pub struct GatewayProxyManagement {
    pub app_name: String,
    pub proxy_list: RefCell<Vec<GatewayProxyInfo>>,
}

impl GatewayProxyManagement {
    fn insert_or_update_proxy_info(&self, dt: &GatewayProxyInfo) {
        let mut found = false;
        let len = self.proxy_list.borrow().len();
        for i in 0..len {
            let bit = &mut self.proxy_list.borrow_mut()[i];
            if bit.app_name == dt.app_name && bit.proxy_address == dt.proxy_address {
                bit.health_status = dt.health_status;
                bit.load_avg = dt.load_avg;
                bit.proxy_token = dt.proxy_token.clone();
                found = true;
            }
        }

        if !found {
            self.proxy_list.borrow_mut().push(dt.clone());
        }
    }

    pub fn add_proxy_info(dt: &GatewayProxyInfo) -> bool {
        let proxy_map = MAP_GATEWAY_PROXY.lock().unwrap();
        let app_name = dt.app_name.clone().unwrap_or_default();
        if proxy_map.borrow_mut().contains_key(&app_name) {
            proxy_map.borrow_mut()[&app_name].insert_or_update_proxy_info(dt);
        } else {
            proxy_map.borrow_mut().insert(
                app_name.clone(),
                GatewayProxyManagement {
                    app_name: app_name.clone(),
                    proxy_list: RefCell::new(vec![dt.clone()]),
                },
            );
        }

        true
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GatewayProxyInfo {
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub proxy_address: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "bool_from_str")]
    pub rewrite_url: Option<bool>, //  need to rewrite url by change the url when the style is prefix
    pub echo_text: Option<String>,
    pub proxy_prefix: Option<String>, // should be provide if style is prefix
    pub proxy_token: Option<String>,
    pub load_avg: Option<f64>, // CPU and memory coverage
    pub load_count: Option<u64>,
    pub health_status: Option<u64>,
    pub living_time: Option<rbatis::DateTimeNative>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GatewayRegisterInfo {
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub server: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "i64_from_str")]
    pub port: Option<i64>,
    #[serde(default)]
    #[serde(deserialize_with = "bool_from_str")]
    pub rewrite_url: Option<bool>, //  need to rewrite url by change the url when the style is prefix
    pub protocol: Option<String>, // default is http
    pub heath_check: Option<String>,
    pub echo_text: Option<String>,
    pub proxy_prefix: Option<String>, // should be provide if style is prefix
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HealthCheckInfo {
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub performance: Option<ChimesPerformanceInfo>,
}

pub fn build_register_info() -> GatewayRegisterInfo {
    let conf = AppConfig::get().lock().unwrap();
    GatewayRegisterInfo {
        app_id: conf.app_id.clone(),
        app_secret: conf.app_secret.clone(),
        server: None,
        port: Some(conf.webserver_conf.port),
        rewrite_url: Some(false),
        protocol: Some("http".to_string()),
        heath_check: Some("/api/v1/healthcheck".to_string()),
        echo_text: Some("Iamliving".to_string()),
        proxy_prefix: None,
    }
}

pub fn build_health_info(with_perf: bool) -> HealthCheckInfo {
    let conf = AppConfig::get().lock().unwrap();
    let perf = if with_perf {
        match ChimesPerformanceInfo::get_performance_info() {
            Ok(p) => Some(p),
            Err(_) => None,
        }
    } else {
        None
    };
    HealthCheckInfo {
        app_id: conf.app_id.clone(),
        app_secret: conf.app_secret.clone(),
        performance: perf,
    }
}

/**
 * Global Gateway Proxy Server
 * 向代理服务器进行注册
 * 或向Gateway进行注册
 */
pub async fn ggp_register_proxy(gr: GatewayRegisterInfo) -> Result<(), ChimesError> {
    let cc = ChimesClient::new_timeout(300);
    let url = AppConfig::get().lock().unwrap().gateway_address.clone();
    if url.is_none() {
        return Err(ChimesError::custom(
            10040,
            "Gateway Server address was not defined.",
        ));
    }
    let fullurl = format!(
        "{}/gateway/api/v1/gateway/register",
        url.unwrap_or_default()
    );
    let res = cc.post(&fullurl, &gr).await?;
    log::info!("Register response: {}", res.clone());
    let ret = serde_json::from_str::<Value>(res.as_str()).unwrap();
    match ret {
        Value::Object(mp) => match mp.get("status") {
            Some(mtt) => match mtt {
                Value::Number(tc) => {
                    if tc.as_i64() == Some(200) || tc.as_i64() == Some(0) {
                        Ok(())
                    } else {
                        Err(ChimesError::custom(
                            10040,
                            format!("Bad response. {}", res.clone()),
                        ))
                    }
                }
                Value::String(tcc) => {
                    if tcc.clone() == *"200" || tcc.clone() == *"0" {
                        Ok(())
                    } else {
                        Err(ChimesError::custom(
                            10040,
                            format!("Bad response. {}", res.clone()),
                        ))
                    }
                }
                _ => Err(ChimesError::custom(
                    10040,
                    format!("Bad response. {}", res.clone()),
                )),
            },
            None => Err(ChimesError::custom(
                10040,
                format!("Bad response. {}", res.clone()),
            )),
        },
        _ => Err(ChimesError::custom(
            10043,
            "Gateway Server response bad format.",
        )),
    }
}

/**
 * Global Gateway Proxy Server
 * 代理服务器的健康检查
 * 时不时的向代理服务器自己的健康情况
 */
pub async fn ggp_health_check() -> Result<(), ChimesError> {
    let cc = ChimesClient::new();
    let url = AppConfig::get().lock().unwrap().gateway_address.clone();
    if url.is_none() {
        return Err(ChimesError::custom(
            10040,
            "Gateway Server address was not defined.",
        ));
    }

    let fullurl = format!("{}/gateway/api/v1/healthcheck", url.unwrap_or_default());
    let perf = build_health_info(true);
    let res = cc.post(&fullurl, &perf).await?;
    log::info!("Healthcheck response: {}", res.clone());
    match serde_json::from_str::<ApiResult<Vec<GatewayProxyInfo>>>(res.as_str()) {
        Ok(lts) => {
            if lts.status == 200 || lts.status == 0 {
                if lts.data.is_some() {
                    for ts in lts.data.unwrap() {
                        let _ = GatewayProxyManagement::add_proxy_info(&ts);
                    }
                }
                Ok(())
            } else {
                Err(ChimesError::custom(
                    10043,
                    format!(
                        "Gateway Server responses warning message. {}",
                        lts.message.clone()
                    ),
                ))
            }
        }
        Err(err) => Err(ChimesError::custom(
            10043,
            format!("Gateway Server response bad format. {}", err),
        )),
    }
}

fn execute_healthcheck(_task: &ProcessTask) -> Pin<Box<dyn Future<Output = ()>>> {
    Box::pin(async move {
        match ggp_health_check().await {
            Ok(_) => {}
            Err(err) => {
                log::info!("Error for health check. {}", err);
            }
        }
    })
}

fn execute_register(_task: &ProcessTask) -> Pin<Box<dyn Future<Output = ()>>> {
    Box::pin(async move {
        let gt = build_register_info();
        match ggp_register_proxy(gt).await {
            Ok(_) => {
                log::info!("Gateway was registered.");
            }
            Err(err) => {
                log::info!("Unable to register in gateway. {}", err);
            }
        }
    })
}

pub async fn ggp_add_health_check_task() {
    let conf = AppConfig::get().lock().unwrap().clone();
    if conf.app_id.is_some() && conf.app_secret.is_some() {
        queue_registry_handler("execute_healthcheck", execute_healthcheck).await;
        let task = ProcessTask::new_with_cookie_fn("health_check", true, 60, "execute_healthcheck");
        get_task_queue().queue_add(task).await;
    }
}

pub async fn ggp_delay_register() {
    let conf = AppConfig::get().lock().unwrap().clone();
    if conf.app_id.is_some() && conf.app_secret.is_some() {
        queue_registry_handler("execute_register", execute_register).await;
        let task = ProcessTask::new_with_cookie_fn("register", false, 10, "execute_register");
        get_task_queue().queue_add(task).await;
    }
}
