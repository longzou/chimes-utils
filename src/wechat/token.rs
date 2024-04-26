use crate::ChimesClient;
use crate::{current_timestamp_secs, json_decode, WechatResult as Result, API_DOMAIN};
use std::{collections::HashMap, sync::Mutex};

use super::errors::WechatError;
// use crate::{json_decode, WechatError};

const WECHAT_CONFIG_KEY_TOKIO: &str = "wechat_config_cache_tokio";
// 默认加载静态全局
lazy_static! {
    pub static ref WECHAT_CONFIG_CACHES_ATT: Mutex<HashMap<String, TokioToken>> =
        Mutex::new(HashMap::new());
}

/// Access Token对象
#[derive(Debug, Clone, Default)]
pub struct TokioAccessToken {
    // 应用类型
    pub app_type: String,
    // 应用id
    pub app_id: String,
    // 密钥
    pub secret: String,
}

/// Token信息
#[derive(Debug, Clone, Default)]
pub struct TokioToken {
    // 访问token
    pub access_token: String,
    // access_token获取时间
    pub create_time: u64,
    // 有效期
    pub expires: i64,
}

impl TokioAccessToken {
    /// 创建一个短信配置实例
    pub fn new(app_type: &str, app_id: &str, secret: &str) -> TokioAccessToken {
        TokioAccessToken {
            app_type: app_type.to_owned(),
            app_id: app_id.to_owned(),
            secret: secret.to_owned(),
        }
    }

    /// 获取微信授权的accessToken
    pub async fn get_access_token(&self, grant_type: &str) -> Result<TokioToken> {
        // 组装请求地址
        let url = format!(
            "{domain}/cgi-bin/token?grant_type={grant_type}&appid={app_id}&secret={secret}",
            domain = API_DOMAIN,
            grant_type = if grant_type.is_empty() {
                "client_credential"
            } else {
                grant_type
            },
            app_id = self.app_id,
            secret = self.secret
        );

        // 调用远程接口
        match ChimesClient::new().get(&url).await {
            Ok(res) => {
                match json_decode(&res) {
                    Ok(data) => {
                        let token = match data["access_token"].as_str() {
                            Some(s) => s.to_owned(),
                            None => return Err(WechatError::msg("access token error")),
                        };

                        // 将Token返出去
                        return Ok(TokioToken {
                            access_token: token,
                            create_time: current_timestamp_secs(),
                            expires: 7200,
                        });
                    }
                    Err(err) => {
                        return Err(err);
                    }
                }
            }
            Err(err) => log::error!("error{:?}", err),
        }

        Err(WechatError::msg("access token is invalid"))
    }

    /// 把字符串对象写入缓存中,并指定有有效期单位秒
    pub async fn set(&mut self, val: TokioToken) {
        let key = WECHAT_CONFIG_KEY_TOKIO;

        let rkey = format!("{}-{}", key, self.app_id.clone());

        let mut cache = WECHAT_CONFIG_CACHES_ATT.lock().unwrap();

        cache.insert(rkey.to_owned(), val);
        // log!("setted config");
    }

    pub fn clear() {
        let mut cache = WECHAT_CONFIG_CACHES_ATT.lock().unwrap();
        cache.clear();
    }

    /// 获取cache中的缓存数据
    pub async fn get(&mut self) -> Option<TokioToken> {
        let key = WECHAT_CONFIG_KEY_TOKIO;
        let appid = self.app_id.clone();
        let rkey = format!("{}-{}", key, appid.clone());
        let cache = WECHAT_CONFIG_CACHES_ATT.lock().unwrap().clone();

        if let Some(mcx) = cache.get(rkey.as_str()) {
            let now = current_timestamp_secs();
            if now < mcx.create_time + mcx.expires as u64 - 1000u64 {
                return Some(mcx.clone());
            }
        }

        drop(cache);

        match self.get_access_token(self.app_type.clone().as_str()).await {
            Ok(access) => {
                self.set(access.clone()).await;
                return Some(access.clone());
            }
            Err(err) => {
                log::info!("get access token error{:?}", err);
            }
        }
        None
    }
}
