use std::cell::RefCell;

use std::{fs::write, sync::Mutex};

use base64::prelude::*;
use futures_util::future::LocalBoxFuture;
use rhai::Scope;
use serde_json::{Map, Value};
use tera::{Context, Tera};

use crate::{get_local_timestamp, ChimesError};

fn base64_decode(base: &str, urlsafe: bool) -> Result<Vec<u8>, ChimesError> {
    let newt = base.replace('\n', "");
    let newt_str = newt.as_str();
    // let eng = base64::prelude::
    if urlsafe {
        match BASE64_URL_SAFE_NO_PAD.decode(newt_str) {
            Ok(vs) => Ok(vs),
            Err(err) => {
                log::info!("Decode URL_SAFE_NO_PAD the base64 with error {}", err);
                match BASE64_URL_SAFE.decode(newt_str) {
                    Ok(vs) => Ok(vs),
                    Err(err) => {
                        log::info!("Decode URL_SAFE the base64 with error {}", err);
                        Err(ChimesError::custom(100010, err.to_string()))
                    }
                }
            }
        }
    } else {
        match BASE64_STANDARD_NO_PAD.decode(newt_str) {
            Ok(vs) => Ok(vs),
            Err(err) => {
                log::info!("Decode STANDARD_NO_PAD the base64 with error {}", err);
                match BASE64_STANDARD.decode(newt_str) {
                    Ok(vs) => Ok(vs),
                    Err(err) => {
                        log::info!("Decode STANDARD the base64 with error {}", err);
                        Err(ChimesError::custom(100010, err.to_string()))
                    }
                }
            }
        }
    }
}

fn base64_encode(content: Vec<u8>) -> String {
    BASE64_STANDARD.encode(content)
}

pub fn write_file(path: &String, content: &Vec<u8>) {
    match write(path, content) {
        Ok(_) => {}
        Err(err) => {
            log::info!("Write file with an error {}", err);
        }
    }
}

pub fn translate_dict(_name: String, _value: String) -> String {
    String::new()
}

pub trait DictTranslate {
    fn translate(&self, name: &str, val: &str) -> String;
    fn reload_all_dicts(&'static self) -> LocalBoxFuture<'static, Result<(), ChimesError>>;
}

pub struct Dummy {}

impl DictTranslate for Dummy {
    fn translate(&self, _name: &str, _val: &str) -> String {
        String::new()
    }

    fn reload_all_dicts(&'static self) -> LocalBoxFuture<'static, Result<(), ChimesError>> {
        Box::pin(async move { Ok(()) })
    }
}

pub struct Translation {
    h: Box<dyn DictTranslate>,
}

impl Translation {
    pub fn new() -> Self {
        Self {
            h: Box::new(Dummy {}),
        }
    }

    pub fn replace(&mut self, dt: impl DictTranslate + 'static) {
        self.h = Box::new(dt);
    }
}

impl Default for Translation {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for Translation {}
unsafe impl Sync for Translation {}

lazy_static! {
    pub static ref FUNCTION_TRANSLATE_DICT: Mutex<RefCell<Translation>> =
        Mutex::new(RefCell::new(Translation::new()));
}

pub fn set_translate_dict_fn(func: impl DictTranslate + 'static) {
    FUNCTION_TRANSLATE_DICT
        .lock()
        .unwrap()
        .borrow_mut()
        .replace(func);
}

pub fn get_translate_dict_fn() -> &'static Translation {
    unsafe { &*FUNCTION_TRANSLATE_DICT.lock().unwrap().as_ptr() }
}

pub async fn reload_translate_dicts() {
    // FUNCTION_TRANSLATE_DICT.lock().unwrap().borrow_mut().h.reload_all_dicts();
    match get_translate_dict_fn().h.reload_all_dicts().await {
        Ok(_) => {
            log::info!("Reload dicts successfully.");
        }
        Err(err) => {
            log::info!("Failed: {} ", err);
        }
    }
}

pub fn script_eval(
    script: &str,
    ctx: &Map<String, Value>,
) -> Result<std::string::String, ChimesError> {
    let mut engine = rhai::Engine::new();
    let mut scope = Scope::new();

    engine.register_fn("timestamp", get_local_timestamp);

    engine.register_fn("snowflake_id", move |prefix: &str| {
        let new_id = rbatis::snowflake::new_snowflake_id();
        format!("{}_{}", prefix, new_id)
    });

    engine.register_fn("base64_encode", move |content: Vec<u8>| {
        base64_encode(content)
    });

    engine.register_fn(
        "base64_decode_std",
        move |content: String| match base64_decode(content.as_str(), false) {
            Ok(ts) => ts,
            Err(_) => vec![],
        },
    );

    engine.register_fn(
        "base64_decode_url",
        move |content: String| match base64_decode(content.as_str(), true) {
            Ok(ts) => ts,
            Err(_) => vec![],
        },
    );

    engine.register_fn("write_file", move |name: &str, content: Vec<u8>| {
        write_file(&name.to_string(), &content);
    });

    engine.register_fn("translate_dict", move |name: &str, val: &str| {
        get_translate_dict_fn().h.translate(name, val)
    });

    let json = Value::Object(ctx.clone()).to_string();

    match engine.parse_json(json.as_str(), true) {
        Ok(dynval) => {
            scope.push("ctx", dynval);
        }
        Err(_) => {
            log::info!("could not convert to rhai::Dynamic. ");
        }
    };

    log::debug!("Script: {}", script);

    let tt = match engine.eval_with_scope::<String>(&mut scope, script) {
        Ok(t) => t,
        Err(err) => {
            log::info!("error on execute the script: {}", err.to_string());
            return Err(ChimesError::custom(210, err.to_string()));
        }
    };
    Ok(tt)
}

pub fn template_eval(script: &str, ctx: Value) -> Result<String, ChimesError> {
    let mut tera = match Tera::new("templates/**/*.tera") {
        Ok(t) => t,
        Err(err) => {
            log::info!(
                "Could not found tera context {}, then use default Tera.",
                err
            );
            // return Err(ChimesError::custom(310, err.to_string()));
            Tera::default()
        }
    };

    let context = match Context::from_serialize(&ctx) {
        Ok(c) => c,
        Err(_) => Context::new(),
    };

    match tera.render_str(script, &context) {
        Ok(text) => Ok(text),
        Err(err) => Err(ChimesError::custom(410, err.to_string())),
    }
}
