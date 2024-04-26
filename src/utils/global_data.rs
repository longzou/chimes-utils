use std::collections::HashMap;

use std::time::SystemTime;

use std::sync::Mutex;

use base64::display::Base64Display;
use base64::prelude::*;
use rand::thread_rng;

use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{PaddingScheme, PublicKey, RsaPrivateKey, RsaPublicKey};
// use serde_derive::{Deserialize, Serialize};
use crate::get_local_timestamp;
use chrono::offset::Local;
use chrono::{DateTime, NaiveDateTime};
use serde::{Deserialize, Deserializer};

use super::AppConfig;

pub struct ValuePaire {
    value: String,
    key: String,
    timestamp: u64,
    expired: u64,
}

lazy_static! {
    pub static ref APP_DATA: Mutex<HashMap<String, ValuePaire>> = Mutex::new(HashMap::new());
}

/**
 * 当这个APP_DATA的大小太大了后，就需要进行resizing操作
 */
pub fn global_app_data_resizing() {
    let dts = APP_DATA.lock().unwrap();
    let len = dts.len();
    let mut keys = vec![];
    if len > 10usize {
        let t = get_local_timestamp();
        let values = dts.values();

        values.for_each(|f| {
            let ts = f.timestamp + f.expired;
            if t >= ts {
                // global_app_data_remove(&f.key);
                keys.push(f.key.clone());
            }
        });
    }
    drop(dts);

    for key in keys {
        global_app_data_remove(&key);
    }
}

#[allow(dead_code)]
pub fn global_app_data_insert(key: &str, val: &str) {
    //let mut hs = APP_DATA.lock().as_mut().unwrap().to_owned();
    //hs.insert(key.to_string(), val.to_string());
    //log::info!("Size Of Hash: {}", hs.len());
    global_app_data_insert_with_expire(key, val, 1000 * 60);
}

#[allow(dead_code)]
pub fn global_app_data_insert_with_expire(key: &str, val: &str, exp: u64) {
    //let mut hs = APP_DATA.lock().as_mut().unwrap().to_owned();
    //hs.insert(key.to_string(), val.to_string());
    //log::info!("Size Of Hash: {}", hs.len());
    let value = ValuePaire {
        value: val.to_owned(),
        key: key.to_owned(),
        timestamp: get_local_timestamp(),
        expired: exp,
    };

    APP_DATA
        .lock()
        .as_mut()
        .unwrap()
        .insert(key.to_string(), value);

    global_app_data_resizing();
}

#[allow(dead_code)]
pub fn global_app_data_remove(key: &String) {
    //let mut hs = APP_DATA.lock().as_mut().unwrap().to_owned();
    //hs.insert(key.to_string(), val.to_string());
    //log::info!("Size Of Hash: {}", hs.len());
    APP_DATA.lock().as_mut().unwrap().remove(key);
}

#[allow(dead_code)]
pub fn global_app_data_get(key: &String) -> Option<String> {
    let dt = APP_DATA.lock().unwrap();
    let cp = dt.get(key);
    if cp.is_none() {
        None
    } else {
        let mpt = cp.unwrap();
        if mpt.expired > 0 {
            let tm = get_local_timestamp();
            if tm > mpt.timestamp + mpt.expired {
                global_app_data_remove(key);
                return None;
            }
        }
        Some(cp.unwrap().value.clone())
    }
}

#[allow(dead_code)]
pub fn rsa_decrypt_by_private_key(token: &String) -> Option<String> {
    let private_key = AppConfig::get()
        .lock()
        .unwrap()
        .to_owned()
        .webserver_conf
        .rsa_password_private_key;

    let bs = match BASE64_STANDARD.decode(private_key) {
        Ok(rs) => rs,
        Err(_) => {
            vec![]
        }
    };

    let priv_key = match RsaPrivateKey::from_pkcs8_der(&bs) {
        Ok(r) => Some(r),
        Err(err) => {
            log::warn!("Decode the Private Key with an error {}", err);
            None
        }
    };

    match priv_key {
        Some(pkey) => {
            let basedecode = match BASE64_STANDARD.decode(token) {
                Ok(ts) => ts,
                Err(_) => vec![],
            };
            let dcode = pkey.decrypt(PaddingScheme::PKCS1v15Encrypt, &basedecode);
            match dcode {
                Ok(rs) => match String::from_utf8(rs) {
                    Ok(text) => Some(text),
                    Err(err) => {
                        log::warn!("Convert to utf8 with an error {}", err);
                        None
                    }
                },
                Err(err) => {
                    log::warn!("Decode the token with an error {}", err.to_string());
                    None
                }
            }
        }
        None => None,
    }
}

#[allow(dead_code)]
pub fn rsa_encrypt_by_public_key(token: &String) -> Option<String> {
    let public_key = AppConfig::get()
        .lock()
        .unwrap()
        .to_owned()
        .webserver_conf
        .rsa_password_public_key;

    let bs = match BASE64_STANDARD.decode(public_key) {
        Ok(rs) => rs,
        Err(_) => {
            vec![]
        }
    };

    let pub_key = match RsaPublicKey::from_public_key_der(&bs) {
        Ok(r) => Some(r),
        Err(err) => {
            log::warn!("Decode the Private Key with an error {}", err);
            None
        }
    };

    match pub_key {
        Some(pkey) => {
            let mut rng = thread_rng();
            let encoded = pkey.encrypt(&mut rng, PaddingScheme::PKCS1v15Encrypt, token.as_bytes());
            match encoded {
                Ok(rs) => {
                    let encodebase = Base64Display::new(&rs, &BASE64_STANDARD).to_string(); // .decode(rs);
                    Some(encodebase)
                }
                Err(err) => {
                    log::warn!("Decode the token with an error {}", err.to_string());
                    None
                }
            }
        }
        None => None,
    }
}

#[derive(Deserialize)]
#[serde(untagged)] // 枚举类型的无标签方式
enum StrOrU64 {
    None,
    String(String),
    U64(u64),
}

#[derive(Deserialize)]
#[serde(untagged)] // 枚举类型的无标签方式
enum StrOrI64 {
    None,
    String(String),
    I64(i64),
}

#[derive(Deserialize)]
#[serde(untagged)] // 枚举类型的无标签方式
enum StrOrF64 {
    None,
    String(String),
    F64(f64),
}

#[derive(Deserialize)]
#[serde(untagged)] // 枚举类型的无标签方式
enum StrOrF32 {
    None,
    String(String),
    F32(f32),
}

#[derive(Deserialize)]
#[serde(untagged)] // 枚举类型的无标签方式
enum StrOrBool {
    String(String),
    I64(i64),
    Bool(bool),
    None,
}

#[allow(dead_code)]
pub fn u64_from_str<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match StrOrU64::deserialize(deserializer)? {
        StrOrU64::String(v) => v.parse().unwrap_or_default(),
        StrOrU64::U64(v) => v,
        StrOrU64::None => 0u64,
    })
}

#[allow(dead_code)]
pub fn i64_from_str<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match StrOrI64::deserialize(deserializer)? {
        StrOrI64::String(v) => match v.parse::<i64>() {
            Ok(st) => Some(st),
            Err(_) => None,
        },
        StrOrI64::I64(v) => Some(v),
        StrOrI64::None => None,
    })
}

#[allow(dead_code)]
pub fn i32_from_str<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match StrOrI64::deserialize(deserializer)? {
        StrOrI64::String(v) => match v.parse::<i64>() {
            Ok(st) => Some(st as i32),
            Err(_) => None,
        },
        StrOrI64::I64(v) => Some(v as i32),
        StrOrI64::None => None,
    })
}

#[allow(dead_code)]
pub fn f64_from_str<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match StrOrF64::deserialize(deserializer)? {
        StrOrF64::String(v) => Some(v.parse().unwrap_or_default()),
        StrOrF64::F64(v) => Some(v),
        StrOrF64::None => None,
    })
}

#[allow(dead_code)]
pub fn f32_from_str<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match StrOrF32::deserialize(deserializer)? {
        StrOrF32::String(v) => v.parse().unwrap_or_default(),
        StrOrF32::F32(v) => v,
        StrOrF32::None => 0.0f32,
    })
}

#[allow(dead_code)]
pub fn bool_from_str<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match StrOrBool::deserialize(deserializer) {
        Ok(t) => match t {
            StrOrBool::String(v) => match v.parse::<bool>() {
                Ok(tf) => Some(tf),
                Err(err) => {
                    log::warn!("Parse erroor {}", err);
                    None
                }
            },
            StrOrBool::I64(v) => Some(v != 0i64),
            StrOrBool::Bool(v) => Some(v),
            StrOrBool::None => Some(false),
        },
        Err(err) => {
            log::warn!("Deserializer erroor {}", err);
            None
        }
    })
}

pub fn num_to_string(n: i64) -> String {
    let base_codec = [
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
        'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '2', '3', '4', '5', '7', '8', '9',
    ];
    let len = base_codec.len() as i64;
    let mut t = n;
    let mut result = "".to_string();
    while t > 0 {
        let idx = (t % len) as usize;
        let ch = base_codec[idx];
        t /= len;
        result.insert(0, ch);
    }
    result
}

pub fn number_to_string(n: i64) -> String {
    let base_codec = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
    let len = base_codec.len() as i64;
    let mut t = n;
    let mut result = "".to_string();
    while t > 0 {
        let idx = (t % len) as usize;
        let ch = base_codec[idx];
        t /= len;
        result.insert(0, ch);
    }
    result
}

pub fn generate_rand_string(len: usize) -> String {
    let mut retkey = "".to_string();

    while retkey.len() < len {
        let rng = rand::random::<u16>();
        let key = num_to_string(rng as i64);
        retkey += key.as_str();
    }

    retkey.chars().take(len).collect()
}

pub fn generate_rand_numberstring(len: usize) -> String {
    let mut retkey = "".to_string();

    while retkey.len() < len {
        let rng = rand::random::<u16>();
        let key = number_to_string(rng as i64);
        retkey += key.as_str();
    }

    retkey.chars().take(len).collect()
}

#[allow(dead_code)]
pub fn num_to_string_v2(n: i64) -> String {
    let base_codec = [
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'p', 'q', 'r', 's', 't',
        'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M',
        'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '2', '3', '4', '5', '6', '7',
        '8', '9',
    ];
    let len = base_codec.len() as i64;
    let mut t = n;
    let mut result = "".to_string();
    while t > 0 {
        let idx = (t % len) as usize;
        let ch = base_codec[idx];
        t /= len;
        result.insert(0, ch);
    }
    result
}
#[allow(dead_code)]
pub fn f32_to_decimal(f: f32) -> Option<rbatis::Decimal> {
    match rbatis::Decimal::from_str(format!("{:.2}", f).as_str()) {
        Ok(r) => Some(r),
        Err(_) => None,
    }
}

#[allow(dead_code)]
pub fn f64_to_decimal(f: f64) -> Option<rbatis::Decimal> {
    println!("f64: value:  {:.2}, {}", f.clone(), f.clone());
    match rbatis::Decimal::from_str(format!("{:.2}", f).as_str()) {
        Ok(r) => Some(r),
        Err(_) => None,
    }
}

#[allow(dead_code)]
pub fn f64_to_decimal_v2(f: f64) -> Option<rbatis::Decimal> {
    println!("f64: value:  {:.2}, {}", f.clone(), f.clone());
    let cff = if f < 0.01f64 { 0.00 } else { f };

    match rbatis::Decimal::from_str(format!("{:.2}", cff).as_str()) {
        Ok(r) => Some(r),
        Err(_) => None,
    }
}

#[allow(dead_code)]
pub fn decimal_to_f32(dc: Option<rbatis::Decimal>) -> f32 {
    dc.map(|r| r.to_string().parse::<f32>().unwrap_or_default())
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn decimal_to_f64(dc: Option<rbatis::Decimal>) -> f64 {
    dc.map(|r| r.to_string().parse::<f64>().unwrap_or_default())
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn decimal_compare(dc: Option<rbatis::Decimal>, dc2: Option<rbatis::Decimal>) -> i64 {
    let f1 = decimal_to_f64(dc);
    let f2 = decimal_to_f64(dc2);

    if f1 == f2 {
        0i64
    } else if f1 > f2 {
        1i64
    } else {
        -1i64
    }
}

#[allow(dead_code)]
pub fn decimal_add(
    dc: Option<rbatis::Decimal>,
    dc2: Option<rbatis::Decimal>,
    ng: bool,
) -> Option<rbatis::Decimal> {
    let f1 = decimal_to_f64(dc);
    let f2 = decimal_to_f64(dc2);

    if ng {
        f64_to_decimal(f1 - f2)
    } else {
        f64_to_decimal(f1 + f2)
    }
}

#[allow(dead_code)]
pub fn make_decimal_negative(dc: Option<rbatis::Decimal>) -> Option<rbatis::Decimal> {
    match dc {
        Some(r) => match r.to_string().parse::<f32>() {
            Ok(t) => f32_to_decimal(-t),
            Err(_) => f32_to_decimal(0f32),
        },
        None => f32_to_decimal(0f32),
    }
}

#[allow(dead_code)]
pub fn int_to_decimal(val: &Option<i64>, mltp: f64) -> Option<rbatis::Decimal> {
    if val.is_none() {
        None
    } else {
        let fx = val.unwrap() as f64;
        f64_to_decimal(fx / mltp)
    }
}

#[allow(dead_code)]
pub fn decimal_mult_decimal(
    val: &Option<rbatis::Decimal>,
    val2: &Option<rbatis::Decimal>,
) -> Option<rbatis::Decimal> {
    f64_to_decimal_v2(decimal_to_f64(val.clone()) * decimal_to_f64(val2.clone()))
}

#[allow(dead_code)]
pub fn decimal_plus_decimal(
    val: &Option<rbatis::Decimal>,
    val2: &Option<rbatis::Decimal>,
) -> Option<rbatis::Decimal> {
    f64_to_decimal_v2(decimal_to_f64(val.clone()) + decimal_to_f64(val2.clone()))
}

#[allow(dead_code)]
pub fn generate_rand_string_v2(len: usize) -> String {
    let mut retkey = "".to_string();

    while retkey.len() < len {
        let rng = rand::random::<u16>();
        let key = num_to_string_v2(rng as i64);
        retkey += key.as_str();
    }

    retkey.chars().take(len).collect()
}

#[allow(dead_code)]
pub fn get_date_string() -> String {
    let now = SystemTime::now();
    let date: DateTime<Local> = now.into();
    let fmt = format!("{}", date.format("%Y%m%d"));
    fmt
}

#[allow(dead_code)]
pub fn format_date_string(dt: &NaiveDateTime) -> String {
    let fmt = format!("{}", dt.format("%Y年%m月%d日 %H:%M"));
    fmt
}

pub fn get_url_encode(c: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
    const FRAGMENT: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'+')
        .add(b'=')
        // .add(b'/')
        ;
    utf8_percent_encode(c, FRAGMENT).to_string()
}

pub fn get_url_encode2(c: &str) -> String {
    urlencoding::encode_binary(c.as_bytes()).into_owned()
}
