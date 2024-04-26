//! copyright © ecdata.cn 2021 - present
//! 获取客服消息内的临时素材。即下载临时的多媒体文件。目前小程序仅支持下载图片文件。
//! created by shaipe 20210228
//!
//! customerServiceMessage.getTempMedia
//! DOC https://developers.weixin.qq.com/miniprogram/dev/api-backend/open-api/customer-message/customerServiceMessage.getTempMedia.html
//!
//! 发送客服消息给用户。
//! customerServiceMessage.send
//! DOC https://developers.weixin.qq.com/miniprogram/dev/api-backend/open-api/customer-message/customerServiceMessage.send.html
//! 下发客服当前输入状态给用户。
//! customerServiceMessage.setTyping
//! 把媒体文件上传到微信服务器。目前仅支持图片。用于发送客服消息或被动回复用户消息。
//! customerServiceMessage.uploadTempMedia
//!
//! 下发小程序和公众号统一的服务消息
//! uniformMessage.send
//!
//! 创建被分享动态消息或私密消息的 activity_id。
//! updatableMessage.createActivityId
//!
//! 修改被分享的动态消息。
//! updatableMessage.setUpdatableMsg
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
use serde::Serialize;
use serde_json::{json, Value};

use crate::ChimesClient;

#[derive(Debug, Eq, PartialEq, Clone, Serialize)]
pub struct WeappTemplateMessage {
    template_id: String,
    page: String,
    form_id: String,
    emphasis_keyword: String,
    data: Value,
}

impl WeappTemplateMessage {
    pub fn new(template_id: &str, page: &str, form_id: &str, emph: &str, data: Value) -> Self {
        Self {
            template_id: template_id.to_owned(),
            page: page.to_owned(),
            form_id: form_id.to_owned(),
            emphasis_keyword: emph.to_owned(),
            data: data.clone(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize)]
pub struct MPTemplateMessage {
    appid: String,
    template_id: String,
    url: String,
    miniprogram: Value,
    data: Value,
}

impl MPTemplateMessage {
    pub fn new(
        app_id: &str,
        template_id: &str,
        url: &str,
        miniprogram: &str,
        pagepath: &str,
        data: &Value,
    ) -> Self {
        Self {
            template_id: template_id.to_owned(),
            appid: app_id.to_owned(),
            url: url.to_owned(),
            miniprogram: json!({"appid": miniprogram.to_owned(), "pagepath": pagepath.to_owned()}),
            data: data.clone(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize)]
pub struct UniformMessage {
    touser: String,
    weapp_template_msg: Option<WeappTemplateMessage>,
    mp_template_msg: MPTemplateMessage,
}

impl UniformMessage {
    pub fn new(
        to_user: &str,
        weapp: &Option<WeappTemplateMessage>,
        mp: &MPTemplateMessage,
    ) -> Self {
        Self {
            touser: to_user.to_owned(),
            weapp_template_msg: weapp.clone(),
            mp_template_msg: mp.clone(),
        }
    }

    #[inline]
    pub async fn send(&self, access_token: &String) -> bool {
        let api_url = format!(
        "https://api.weixin.qq.com/cgi-bin/message/wxopen/template/uniform_send?access_token={}",
        access_token
    );
        // let mut params = HashMap::new();
        // params.insert("touser".to_string(), to_user.as_ref().to_string());
        // params.insert("msgtype".to_string(), msg_type.as_ref().to_string());
        // params.insert(msg_type.as_ref().to_string(), content.as_ref().to_string());
        let params = json!(self);
        // let params=format!(r#"{{
        //     "touser":"{}",
        //     "msgtype":"{}",
        //     "{}":{{"content":"{}"}}
        // }}"#,to_user.as_ref().to_string(),
        // msg_type.as_ref().to_string(),
        // msg_type.as_ref().to_string(),
        // content.as_ref().to_string());

        println!(
            "send Unionform Message url:: {} content :: {:?}",
            api_url, params
        );

        match ChimesClient::new().post(&api_url, &params).await {
            Ok(v) => {
                println!("success {:?}", v);
                !v.contains("access_token expired")
            }
            Err(e) => {
                println!("error {:?}", e);
                false
            }
        }
    }
}
