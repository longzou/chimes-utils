use std::fs::File;

// use self::errors::WechatError;

// 请求默认AGENT
pub const WECHAT_DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/70.0.3534.4 Safari/537.36";

/// 微信接口域名
pub const API_DOMAIN: &str = "https://api.weixin.qq.com";

mod errors;
mod message;
mod token;

pub use errors::*;
pub use message::*;
pub use token::*;
pub type WechatResult<T> = Result<T, WechatError>;

#[inline]
pub fn json_decode(data: &str) -> WechatResult<serde_json::Value> {
    let obj: serde_json::Value = match serde_json::from_str(data) {
        Ok(decoded) => decoded,
        Err(ref e) => {
            return Err(WechatError::custom(-3, format!("Json decode error: {}", e)));
        }
    };
    let dic = obj.as_object().unwrap();
    let code = if dic.contains_key("errcode") {
        "errcode"
    } else {
        "code"
    };

    let code = obj[code].as_i64().unwrap_or_default();
    if code != 0 {
        let msg: String = if dic.contains_key("msg") {
            obj["msg"].to_string()
        } else {
            obj["errmsg"].to_string()
        };
        return Err(WechatError::custom(code as i32, msg));
    }
    println!("obj====={:?}", obj);
    Ok(obj)
}

/// 微们接口平台类型
// #[derive(Serialize, Deserialize, Debug, Clone)]
#[derive(Debug, Clone)]
pub enum PlatformType {
    OfficialAccount, // 公众号
    OpenPlatfrom,    // 开放平台
    MiniProgram,     // 小程序
}

/// 微信sdk配置
#[derive(Debug, Clone)]
pub struct WechatConfig {
    pub app_id: String,         // 应用id
    pub offical_appid: String,  // 关联公众号的AppId，用这个来发送消息
    pub secret: String,         // 密钥
    pub token: String,          // token,在接口配置时填写的token,用于sigine验证
    pub platform: PlatformType, // 配置的平台类型
    // pub msg_type: MessageFormat,    // 消息格式
    // pub encrypt_mode: EncryptMode   // 加密方式
    pub mch_id: String,      //商户id
    pub private_key: String, //商户证书私钥
    pub certificate: String, //商户证书路径
    pub secret_key: String,  //API 秘钥
    pub notify_url: String,
    pub refund_notify_url: String,
}

impl WechatConfig {
    /// 设置配置
    pub fn load(params: serde_json::Value) -> WechatResult<WechatConfig> {
        let _conf = WechatConfig {
            app_id: params["app-id"].as_str().unwrap_or_default().to_string(),
            offical_appid: params["offical-appid"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            secret: params["secret"].as_str().unwrap_or_default().to_string(),
            token: params["token"].as_str().unwrap_or_default().to_string(),
            platform: PlatformType::MiniProgram,
            mch_id: params["mch_id"].as_str().unwrap_or_default().to_string(),
            private_key: params["private_key"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            certificate: params["certificate"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            secret_key: params["secret_key"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            notify_url: params["notify-url"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            refund_notify_url: if params["refund-notify-url"].as_str().is_none() {
                params["notify-url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            } else {
                params["refund-notify-url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            },
        };
        Ok(_conf)
    }

    /// 加载yml配置文件
    pub fn load_yaml(conf_path: &str) -> WechatResult<WechatConfig> {
        use yaml_rust::yaml;
        // open file
        let mut f = match File::open(conf_path) {
            Ok(f) => f,
            Err(e) => {
                return Err(WechatError::custom(4004, format!("{}", e)));
            }
        };
        let mut s = String::new();
        use std::io::Read;
        match f.read_to_string(&mut s) {
            Ok(s) => s,
            Err(e) => {
                return Err(WechatError::custom(
                    4004,
                    format!("Error Reading file: {}", e),
                ));
            }
        };
        // f.read_to_string(&mut s).unwrap(); // read file content to s
        // load string to yaml loader
        // println!("Loaded {}", s);
        let docs = yaml::YamlLoader::load_from_str(&s).unwrap();
        // get first yaml hash doc
        let _yaml_doc = &docs[0];
        // get server value
        // let server = yaml_doc["weapp"].clone();

        Ok(WechatConfig::default())
    }
}

/// 默认配置项
impl Default for WechatConfig {
    fn default() -> Self {
        WechatConfig {
            app_id: String::new(),
            offical_appid: String::new(),
            secret: String::new(),
            token: String::new(),
            platform: PlatformType::MiniProgram,
            mch_id: "".to_string(),
            private_key: "".to_string(),
            certificate: "".to_string(),
            secret_key: "".to_string(),
            notify_url: "".to_string(),
            refund_notify_url: "".to_string(),
            // msg_type: MessageFormat::Json,
            // encrypt_mode: EncryptMode::Plaintext
        }
    }
}
