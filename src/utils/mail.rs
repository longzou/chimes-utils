use crate::{global_app_data_resizing, AppConfig};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::SmtpTransport;
use lettre::{Message, Transport};
use std::mem::MaybeUninit;
use std::sync::{Arc, Once};
use std::time::Duration;
use tokio::sync::{Notify, RwLock};

#[derive(Debug, Clone, Default)]
pub struct EmailBody {
    pub email_receiver: String,
    pub mine_email: String,
    pub subject: String,
    pub content: String,
    pub html_email: bool,
}

impl EmailBody {
    fn send(&self, mailer: &mut SmtpTransport) {
        send_email(
            mailer,
            &self.email_receiver,
            &self.mine_email,
            &self.subject,
            &self.content,
            self.html_email,
        );
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmailServerConfig {
    pub smtp_server: String,
    pub password: String,
    pub port: String,
    pub mine_email: String,
    pub ssl: bool,
}
#[derive(Debug, Clone, Default)]
pub struct EmailSendingQueue {
    queue: Vec<EmailBody>,
    client_conf: EmailServerConfig,
    started: bool,
    notify: Arc<Notify>,
    lock: Arc<RwLock<u32>>,
}

impl EmailSendingQueue {
    pub fn new() -> Self {
        Self {
            queue: vec![],
            client_conf: EmailServerConfig {
                smtp_server: String::new(),
                password: String::new(),
                port: String::new(),
                mine_email: String::new(),
                ssl: false,
            },
            started: false,
            notify: Arc::new(Notify::new()),
            lock: Arc::new(RwLock::new(2)),
        }
    }

    pub fn config(
        &mut self,
        smtp_server: String,
        password: String,
        port: String,
        mine_email: String,
        ssl: bool,
    ) {
        self.client_conf.smtp_server = smtp_server;
        self.client_conf.password = password.clone();
        self.client_conf.mine_email = mine_email.clone();
        self.client_conf.port = port;
        self.client_conf.ssl = ssl;
    }

    pub async fn queue_send(&mut self, emb: &EmailBody) {
        let ck = self.lock.clone();
        let mut clemb = emb.clone();
        clemb.mine_email = self.client_conf.mine_email.clone();
        let mu = ck.write().await;
        self.queue.push(clemb);
        drop(mu);
        self.notify.notify_one();
    }

    /**
     * 启动邮件发送队列
     */
    fn start(&'static mut self) {
        if !self.started {
            self.started = true;
            start_email_queue(self);
        }
    }

    /**
     * 关闭邮件发送队列
     */
    fn shutdown(&mut self) {
        if self.started {
            self.started = false;
            self.notify.notify_waiters();
            // waiting for shutdown
        }
    }
}

fn start_email_queue(msq: &'static mut EmailSendingQueue) {
    tokio::spawn(async move {
        let creds = Credentials::new(
            msq.client_conf.mine_email.clone(),
            msq.client_conf.password.clone(),
        );

        log::info!("The email sending queue was created and processing.");

        let notified = msq.notify.clone();
        let rclock = msq.lock.clone();

        while msq.started {
            // Open connection to Gmail
            let mut fst = msq.queue.pop();
            if fst.is_some() {
                let mut mailer = SmtpTransport::starttls_relay(&msq.client_conf.smtp_server)
                    .unwrap()
                    .credentials(creds.clone())
                    .build();

                while fst.is_some() {
                    let email = fst.unwrap();
                    email.send(&mut mailer);

                    {
                        let mu = rclock.read().await;
                        fst = msq.queue.pop();
                        drop(mu);
                    }
                }
            } else {
                match tokio::time::timeout(Duration::from_secs(60), notified.notified()).await {
                    Ok(_) => {
                        log::info!("Received a new email notification. The loop will continues.");
                    }
                    Err(err) => {
                        global_app_data_resizing();
                        log::info!("Timeout {}", err);
                    }
                }
            }
        }
    });
}

fn send_email(
    mailer: &mut SmtpTransport,
    email_receiver: &str,
    mine_email: &str,
    subject: &str,
    content: &str,
    html_email: bool,
) {
    let from: Mailbox = match mine_email.parse() {
        Ok(t) => t,
        Err(err) => {
            log::info!("Error for parse mine email address. {}", err);
            return;
        }
    };
    let to: Mailbox = match email_receiver.parse() {
        Ok(t) => t,
        Err(err) => {
            log::info!("Error for parse mine email address. {}", err);
            return;
        }
    };
    let email_build = if html_email {
        Message::builder()
            .from(from)
            .to(to)
            .subject(subject.to_string())
            .body(content.to_string())
            .unwrap()
    } else {
        Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .body(content.to_string())
            .unwrap()
    };

    match mailer.send(&email_build) {
        Ok(_) => {
            log::info!("Mail was sent.");
        }
        Err(err) => {
            log::info!("Could not send email: {:?}", err);
        }
    }
}

pub fn get_email_queue() -> &'static mut EmailSendingQueue {
    // 使用MaybeUninit延迟初始化
    static mut STATIC_ESQ: MaybeUninit<EmailSendingQueue> = MaybeUninit::uninit();
    // Once带锁保证只进行一次初始化
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        // CONF = 1u64;
        let conf = AppConfig::get().lock().unwrap().to_owned();
        let emailconf = conf.email_conf.clone();

        async_std::task::block_on(async {
            let mut esq = EmailSendingQueue::new();
            esq.config(
                emailconf.smtp_server,
                emailconf.password,
                emailconf.port,
                emailconf.mine_email,
                emailconf.ssl,
            );
            STATIC_ESQ.as_mut_ptr().write(esq);
        });
    });
    unsafe { &mut *STATIC_ESQ.as_mut_ptr() }
}

pub fn start_email_queue_thread() {
    get_email_queue().start();
}

pub fn stop_email_queue_thread() {
    get_email_queue().shutdown();
}
