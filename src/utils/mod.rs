use chrono::{DateTime, Local};
use log::LevelFilter;
use rbatis::DriverType;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::mem::MaybeUninit;
use std::str::FromStr;
use std::sync::{Mutex, Once};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
// use openssl::hash::{DigestBytes, MessageDigest};
use openssl::sha::Sha256;
use rbatis::log::LogPlugin;
use rbatis::rbatis::Rbatis;
use serde_derive::{Deserialize, Serialize};

mod rbatis_dynamical;
pub use rbatis_dynamical::*;

mod redis_client;
pub use redis_client::*;
mod r2d2_pool;

mod actix_client;
pub use actix_client::*;

mod script_engine;
pub use script_engine::*;

mod gpt_sdk;
pub use gpt_sdk::*;

mod common_search;
pub use common_search::*;

#[cfg(windows)]
mod windows_service_utils;

#[cfg(windows)]
mod windows_performance;

use serde_json::Value;
#[cfg(windows)]
pub use windows_service_utils::*;

#[cfg(not(target_os = "windows"))]
mod linux_performance;

mod performance;
pub use performance::*;

#[allow(dead_code)]
pub fn get_local_timestamp() -> u64 {
    let now = SystemTime::now();
    let date: DateTime<Local> = now.into();
    date.timestamp_millis() as u64
}

#[allow(dead_code)]
pub fn get_multiple_rbatis(url: &str) -> &'static Rbatis {
    // 使用MaybeUninit延迟初始化
    static mut STATIC_MULTI_RB: MaybeUninit<HashMap<String, Rbatis>> = MaybeUninit::uninit();
    // Once带锁保证只进行一次初始化
    static ONCE_HASH: Once = Once::new();

    ONCE_HASH.call_once(|| unsafe {
        STATIC_MULTI_RB.as_mut_ptr().write(HashMap::new());
    });

    unsafe {
        if (*STATIC_MULTI_RB.as_mut_ptr()).contains_key(url) {
            let hashmap = &*STATIC_MULTI_RB.as_mut_ptr();
            &hashmap[url]
        } else {
            async_std::task::block_on(async {
                log::info!("Call the block on to create the sql connection.");
                let mut rb = Rbatis::new();
                match rb.link(url).await {
                    Ok(_) => {
                        log::info!(
                            "Database {} was connected. Rbatis was initialized successfully.",
                            url
                        );
                    }
                    Err(err) => {
                        log::warn!("Error: {}", err);
                    }
                };
                rb.add_sql_intercept(MultipleDatabaseIntercepter());
                (*STATIC_MULTI_RB.as_mut_ptr()).insert(url.to_string(), rb);
            });
            let hashmap = &*STATIC_MULTI_RB.as_mut_ptr();
            &hashmap[url]
        }
    }
}

#[allow(dead_code)]
pub async fn get_multiple_rbatis_async(url: &str) -> &'static Rbatis {
    // 使用MaybeUninit延迟初始化
    static mut STATIC_MULTI_RB: MaybeUninit<HashMap<String, Rbatis>> = MaybeUninit::uninit();
    // Once带锁保证只进行一次初始化
    static ONCE_HASH: Once = Once::new();

    ONCE_HASH.call_once(|| unsafe {
        STATIC_MULTI_RB.as_mut_ptr().write(HashMap::new());
    });

    unsafe {
        if (*STATIC_MULTI_RB.as_mut_ptr()).contains_key(url) {
            let hashmap = &*STATIC_MULTI_RB.as_mut_ptr();
            &hashmap[url]
        } else {
            log::info!("Call the block on to create the sql connection.");
            let mut rb = Rbatis::new();
            match rb.link(url).await {
                Ok(_) => {
                    log::info!(
                        "Database {} was connected. Rbatis was initialized successfully.",
                        url
                    );
                }
                Err(err) => {
                    log::warn!("Error: {}", err);
                }
            };
            rb.add_sql_intercept(MultipleDatabaseIntercepter());
            (*STATIC_MULTI_RB.as_mut_ptr()).insert(url.to_string(), rb);
            let hashmap = &*STATIC_MULTI_RB.as_mut_ptr();
            &hashmap[url]
        }
    }
}

#[allow(dead_code)]
pub fn get_rbatis() -> &'static Rbatis {
    // 使用MaybeUninit延迟初始化
    static mut STATIC_RB: MaybeUninit<Rbatis> = MaybeUninit::uninit();
    // Once带锁保证只进行一次初始化
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        // CONF = 1u64;
        let conf = AppConfig::get().lock().unwrap().to_owned();
        let url = conf.db_conf.url.clone();

        async_std::task::block_on(async {
            let mut rb = Rbatis::new();
            match rb.link(&url).await {
                Ok(_) => {
                    log::info!("Database was connected. Rbatis was initialized successfully.");
                }
                Err(err) => {
                    log::warn!("Error: {}", err);
                }
            };
            let rb_log = rbatis::log::RbatisLogPlugin::default();
            rb_log.set_level_filter(conf.db_conf.logger_level);
            rb.set_log_plugin(rb_log);
            rb.add_sql_intercept(MultipleDatabaseIntercepter());
            STATIC_RB.as_mut_ptr().write(rb);
        });
    });
    unsafe { &*STATIC_RB.as_ptr() }
}

#[derive(Debug, Clone)]
pub struct MultipleDatabaseIntercepter();

impl rbatis::intercept::SqlIntercept for MultipleDatabaseIntercepter {
    fn do_intercept(
        &self,
        rb: &Rbatis,
        sql: &mut String,
        _args: &mut Vec<rbson::Bson>,
        _is_prepared_sql: bool,
    ) -> Result<(), rbatis::core::Error> {
        log::debug!("Origal: {}", sql.clone());
        let m_sql = rbatis_compatible_sql(rb, sql.as_str());
        log::debug!("Modify: {}", m_sql.clone());
        *sql = m_sql;
        Ok(())
    }
}

/**
 * 根据数据库类型来产生，对应的兼容的SQL查询语句
 * 原始参考SQL是MySQL
 * 如目标数据库类型是PostgreSQL，就将其转换成为$1这样的参数
 */
pub fn rbatis_compatible_sql(rb: &Rbatis, sql: &str) -> String {
    match rb.driver_type() {
        Ok(driver_type) => match driver_type {
            DriverType::Postgres => {
                let mut sql_out = String::new();
                let mut spl = sql.split('?');
                let mut i = 1;
                let count = spl.clone().count();
                loop {
                    let t = spl.next();
                    if t.is_some() {
                        sql_out.push_str(t.unwrap());
                        if i < count {
                            sql_out.push_str(format!("${}", i).as_str());
                        }
                    } else {
                        break;
                    }
                    i += 1;
                }
                sql_out
            }
            _ => sql.to_owned(),
        },
        Err(_) => sql.to_owned(),
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub db_conf: DatabaseConfig,
    pub webserver_conf: WebServerConfig,
    pub email_conf: EmailServerConfig,
    pub redis_conf: Option<RedisConfig>,
    pub gateway_address: Option<String>,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub logger_level: LevelFilter,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            db_conf: Default::default(),
            webserver_conf: Default::default(),
            email_conf: Default::default(),
            redis_conf: Default::default(),
            gateway_address: Default::default(),
            app_id: Default::default(),
            app_secret: Default::default(),
            logger_level: LevelFilter::Info,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RedisPoolConfig {
    pub connection_timeout: u64,
    pub max_size: u32,
    pub mini_idel: u32,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum InstanceType {
    Single,
    Cluster,
}

#[derive(Debug, Clone, Default)]
pub struct RedisConfig {
    pub urls: Vec<String>,
    pub database: i64,
    pub username: Option<String>,
    pub password: Option<String>,
    pub instance_type: Option<InstanceType>,
    pub pool: RedisPoolConfig,
}

impl RedisConfig {
    #[allow(dead_code)]
    fn instance_type_default() -> InstanceType {
        InstanceType::Single
    }
}

#[derive(Debug, Clone, Default)]
pub struct WebServerConfig {
    pub port: i64,
    pub rsa_key: String,
    pub rsa_cert: String,
    pub rsa_password_private_key: String,
    pub rsa_password_public_key: String,
    pub upload_store_path: String,
    pub resources_path: String,
    pub upload_temp_path: String,
    pub access_url_prefix: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub logger_level: LevelFilter,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Default::default(),
            logger_level: LevelFilter::Info,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct FaceDbConfig {
    pub url: String,
}

impl AppConfig {
    pub fn get() -> &'static Mutex<AppConfig> {
        // 使用MaybeUninit延迟初始化
        static mut CONF: MaybeUninit<Mutex<AppConfig>> = MaybeUninit::uninit();
        // Once带锁保证只进行一次初始化
        static ONCE: Once = Once::new();

        ONCE.call_once(|| unsafe {
            CONF.as_mut_ptr().write(Mutex::new(AppConfig {
                db_conf: DatabaseConfig {
                    url: "".to_string(),
                    logger_level: LevelFilter::Info,
                },
                webserver_conf: WebServerConfig {
                    port: 10089i64,
                    rsa_cert: String::new(),
                    rsa_key: String::new(),
                    rsa_password_private_key: String::new(),
                    rsa_password_public_key: String::new(),
                    upload_store_path: String::new(),
                    upload_temp_path: String::new(),
                    access_url_prefix: String::new(),
                    resources_path: String::new(),
                },
                email_conf: EmailServerConfig::default(),
                redis_conf: None,
                gateway_address: None,
                app_id: None,
                app_secret: None,
                logger_level: LevelFilter::Info,
            }));
        });
        unsafe { &*CONF.as_ptr() }
    }

    pub fn init(conf_path: &str) {
        AppConfig::get().lock().unwrap().load_yaml(conf_path);
    }

    pub fn load_yaml(&mut self, conf_path: &str) {
        use yaml_rust::yaml;
        // open file
        let mut f = match File::open(conf_path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let mut s = String::new();
        let _ = f.read_to_string(&mut s).is_ok();
        // f.read_to_string(&mut s).unwrap(); // read file content to s
        // load string to yaml loader
        let docs = yaml::YamlLoader::load_from_str(&s).unwrap();
        // get first yaml hash doc
        // get server value
        // let server = yaml_doc["weapp"].clone();
        let doc = &docs[0];
        let db = &doc["database"];
        let web = &doc["webserver"];
        let email = &doc["email"];
        let redis = &doc["redis"];
        let reg = &doc["registry"];

        let log_level = doc["log-level"]
            .as_str()
            .map(|s| match LevelFilter::from_str(s) {
                Ok(ll) => ll,
                Err(_) => LevelFilter::Info,
            })
            .unwrap_or_else(|| LevelFilter::Info);

        let gatewaddr = reg["gateway_address"].as_str().map(|s| s.to_string());
        let app_id = reg["app_id"].as_str().map(|s| s.to_string());
        let app_secret = reg["app_secret"].as_str().map(|s| s.to_string());

        let dbconf = DatabaseConfig {
            url: db["url"].as_str().map(|s| s.to_owned()).unwrap_or_default(),
            logger_level: if let Some(l) = db["log-level"].as_str() {
                match LevelFilter::from_str(l) {
                    Ok(level) => level,
                    Err(_) => LevelFilter::Info,
                }
            } else {
                LevelFilter::Info
            },
        };

        let webconf = WebServerConfig {
            port: web["port"].as_i64().unwrap_or(10089i64),
            rsa_key: web["rsa_key"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            rsa_cert: web["rsa_cert"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            rsa_password_private_key: web["rsa_password_private_key"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            rsa_password_public_key: web["rsa_password_public_key"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            upload_store_path: web["upload-store-path"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            upload_temp_path: web["upload-temp-path"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            resources_path: web["resources-path"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            access_url_prefix: web["access-url-prefix"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
        };
        let emailconf = EmailServerConfig {
            smtp_server: email["smtp-server"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            mine_email: email["account"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            password: email["password"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            port: email["port"]
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_default(),
            ssl: email["ssl"].as_bool().unwrap_or_default(),
        };

        let redis_conf = if !redis.is_null() {
            let pool = &redis["pool"];
            Some(RedisConfig {
                urls: if let Some(s) = redis["urls"].as_vec() {
                    let mut u = vec![];
                    for ps in s.clone() {
                        if let Some(st) = ps.as_str() {
                            u.push(st.to_string());
                        }
                    }
                    u
                } else {
                    vec![]
                },
                database: redis["database"].as_i64().unwrap_or_default(),
                username: redis["username"].as_str().map(|s| s.to_owned()),
                password: redis["password"].as_str().map(|s| s.to_owned()),
                instance_type: if let Some(s) = redis["instance-type"].as_str() {
                    if s == "cluster" {
                        Some(InstanceType::Cluster)
                    } else {
                        Some(InstanceType::Single)
                    }
                } else {
                    Some(InstanceType::Single)
                },
                pool: RedisPoolConfig {
                    connection_timeout: pool["connection_timeout"].as_i64().unwrap_or_default()
                        as u64,
                    max_size: pool["max_size"].as_i64().unwrap_or_default() as u32,
                    mini_idel: pool["mini_idel"].as_i64().unwrap_or_default() as u32,
                },
            })
        } else {
            None
        };

        self.db_conf = dbconf;
        self.webserver_conf = webconf;
        self.email_conf = emailconf;
        self.redis_conf = redis_conf;
        self.gateway_address = gatewaddr;
        self.app_id = app_id;
        self.app_secret = app_secret;
        self.logger_level = log_level;
    }
}

#[cfg(unix)]
pub fn set_file_permission(fcp: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(mode);
    match std::fs::set_permissions(fcp.clone(), permissions) {
        Ok(_) => {}
        Err(err) => {
            log::warn!("Error on set permission: {}", err);
        }
    };
}

#[cfg(windows)]
pub fn set_file_permission(fcp: &str, mode: u32) {
    log::debug!(
        "Updating the permission for {} to {} is ignored.",
        fcp,
        mode
    );
}

/// 获取当前时间戮
pub fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

#[allow(dead_code)]
pub fn text_insert_br(oldtext: Option<String>) -> Option<String> {
    if oldtext.is_none() {
        None
    } else {
        let text = oldtext.unwrap();
        let mut newtext = String::new();
        let lst = text.lines().collect::<Vec<&str>>();
        for stx in 0..lst.len() {
            let sxtr = lst[stx];
            newtext.push_str(sxtr);
            if stx < lst.len() - 1 {
                newtext.push_str("<br />");
            }
        }
        Some(newtext)
    }
}

pub fn file_size_format(len: usize) -> String {
    let t = len as f64 / (1024f64 * 1024f64 * 1024f64 * 1024f64);
    if t >= 1.00f64 {
        return format!("{:.02}TB", t);
    }
    let g = len as f64 / (1024f64 * 1024f64 * 1024f64);
    if g >= 1.00f64 {
        return format!("{:.02}GB", g);
    }
    let m = len as f64 / (1024f64 * 1024f64);
    if m >= 1.00f64 {
        return format!("{:.02}MB", m);
    }
    let k = len as f64 / 1024f64;
    if k >= 1.00f64 {
        return format!("{:.02}KB", k);
    }

    format!("{} B", len)
}

mod mail;
pub use mail::*;

mod queue;
pub use queue::*;

mod global_data;
pub use global_data::*;

#[allow(dead_code)]
pub fn parse_query(query_string: &str) -> HashMap<String, String> {
    if query_string.is_empty() {
        return HashMap::new();
    }
    let q_a: Vec<&str> = query_string.split('&').collect();
    let mut res: HashMap<String, String> = HashMap::new();
    use percent_encoding::percent_decode;
    for s in q_a {
        // let ss: &str = s;
        let kv: Vec<&str> = s.split('=').collect();
        let kvalue = percent_decode(kv[1].as_bytes()).decode_utf8().unwrap();
        res.insert(kv[0].to_string(), kvalue.to_string());
    }
    res
}

#[allow(dead_code)]
pub fn parse_query_as_value(query_string: &str) -> Value {
    if query_string.is_empty() {
        return Value::Null;
    }
    let q_a: Vec<&str> = query_string.split('&').collect();
    let mut res: HashMap<String, String> = HashMap::new();
    use percent_encoding::percent_decode;
    for s in q_a {
        // let ss: &str = s;
        let kv: Vec<&str> = s.split('=').collect();
        let kvalue = percent_decode(kv[1].as_bytes()).decode_utf8().unwrap();
        res.insert(kv[0].to_string(), kvalue.to_string());
    }

    match serde_json::to_value(res) {
        Ok(tv) => tv,
        Err(_) => Value::Null,
    }
}

#[allow(dead_code)]
pub fn get_hash_value(query_params: &HashMap<String, String>, key: &str) -> String {
    match query_params.get(key) {
        Some(val) => val.clone(),
        None => "".to_owned(),
    }
}

#[allow(dead_code)]
pub fn calc_file_hash(filename: &str) -> Option<String> {
    let mut file = match File::open(filename) {
        Ok(file) => file,
        Err(err) => {
            log::info!("error for open file {} with err {}", filename, err);
            return None;
        }
    };

    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(ts) => {
                if ts == 1024 * 1024 {
                    hasher.update(&buffer);
                } else if ts > 0 {
                    let lesbuf = &buffer[0..ts];
                    hasher.update(lesbuf);
                } else {
                    break;
                }
            }
            Err(err) => {
                log::info!("read file {} with err {}", filename, err);
                return None;
            }
        };
    }

    let result = hasher.finish();
    Some(hex::encode(result))
}

pub fn datetime_to_date(dt: &Option<rbatis::DateTimeNative>) -> Option<rbatis::DateNative> {
    dt.as_ref().map(|t| rbatis::DateNative { inner: t.date() })
}

pub fn pure_datetime_to_date(dt: &rbatis::DateTimeNative) -> rbatis::DateNative {
    rbatis::DateNative { inner: dt.date() }
}

pub fn datetime_diff(last: &rbatis::DateTimeNative, prev: &rbatis::DateTimeNative) -> i64 {
    last.and_utc().timestamp() - prev.and_utc().timestamp()
}
