use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::{Arc, Once};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::task::LocalSet;

use crate::{current_timestamp_secs, generate_rand_numberstring, global_app_data_resizing};

type HandlerFunc = fn(&ProcessTask) -> Pin<Box<dyn Future<Output = ()>>>;

lazy_static! {
    pub static ref HANDLER_FUNC_REGISTRY: Mutex<HashMap<String, HandlerFunc>> =
        Mutex::new(HashMap::new());
}

#[derive(Clone)]
pub struct ProcessTask {
    // handler: Rc<&'static dyn TaskHandler>,
    task_id: String,
    handler_type: String,
    cyclicity: bool,
    last_time: RefCell<u64>,
    period: u64,
    pub cookie: Option<String>,
}

impl ProcessTask {
    pub fn new(h: bool, p: u64, ck: &str, hfn: &str) -> Self {
        Self {
            task_id: generate_rand_numberstring(16),
            cyclicity: h,
            period: p,
            last_time: RefCell::new(current_timestamp_secs()),
            handler_type: hfn.to_owned(),
            cookie: Some(ck.to_owned()),
        }
    }

    pub fn new_with_cookie_fn(ck: &str, cycle: bool, p: u64, hfn: &str) -> Self {
        Self {
            task_id: generate_rand_numberstring(16),
            cyclicity: cycle,
            period: p,
            last_time: RefCell::new(current_timestamp_secs()),
            handler_type: hfn.to_owned(),
            cookie: Some(ck.to_owned()),
        }
    }
}

unsafe impl Send for ProcessTask {}

unsafe impl Sync for ProcessTask {}

#[derive(Clone)]
pub struct ProcessQueue {
    queue: Vec<ProcessTask>,
    started: bool,
    max_tasks: usize,
    waiting_timeout: u64,
    notify: Arc<Notify>,
    lock: Arc<RwLock<u32>>,
}

unsafe impl Send for ProcessQueue {}

unsafe impl Sync for ProcessQueue {}

pub async fn queue_registry_handler(handler_type: &str, fun: HandlerFunc) {
    let mut h = HANDLER_FUNC_REGISTRY.lock().await;
    h.insert(handler_type.to_owned(), fun);
}

impl ProcessQueue {
    pub fn new() -> Self {
        Self {
            queue: vec![],
            started: false,
            max_tasks: 1000usize,
            waiting_timeout: 30,
            notify: Arc::new(Notify::new()),
            lock: Arc::new(RwLock::new(2)),
        }
    }

    pub fn process_task(&self, rt: &Runtime, task: &ProcessTask) {
        let local = LocalSet::new();
        local.block_on(rt, async move {
            let mlock = HANDLER_FUNC_REGISTRY.lock().await;
            let hfunc = mlock.get(&task.handler_type);
            if hfunc.is_some() {
                let func = hfunc.unwrap();
                func(task).await;
            }
        });
    }

    pub async fn queue_add(&mut self, emb: ProcessTask) {
        let ck = self.lock.clone();
        // let mut clemb = emb.clone();
        let mu = ck.write().await;
        self.queue.push(emb);
        drop(mu);
        self.notify.notify_one();
    }

    pub async fn queue_remove(&mut self, emb: &ProcessTask) {
        let ck = self.lock.clone();
        // let mut clemb = emb.clone();
        let mu = ck.write().await;
        let len = self.queue.len();
        for i in 0..len {
            if emb.task_id == self.queue[i].task_id {
                self.queue.remove(i);
                break;
            }
        }
        drop(mu);
        // self.notify.notify_one();
    }
    /**
     * 启动邮件发送队列
     */
    fn start(&'static mut self) {
        if !self.started {
            self.started = true;
            start_queue_thread(self);
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

    fn notify(&self) {
        if self.started {
            self.notify.notify_waiters();
        }
    }

    fn count(&self) -> usize {
        self.queue.len()
    }

    fn is_queue_full(&self) -> bool {
        self.queue.len() >= self.max_tasks
    }
}

impl Default for ProcessQueue {
    fn default() -> Self {
        Self::new()
    }
}

fn start_queue_thread(msq: &'static mut ProcessQueue) {
    // async_std::task::spawn(
    let crt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2usize)
        .enable_all()
        .build()
        .unwrap();

    async_std::task::spawn(async move {
        log::info!("The queue was created and processing.");
        let notified = msq.notify.clone();
        let rclock = msq.lock.clone();
        // let crt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        while msq.started {
            // Open connection to Gmail
            let current_time = current_timestamp_secs();
            let mu = rclock.read().await;
            let mut should_remove_tasks = vec![];

            for ct in msq.queue.clone() {
                // log::info!("Current task: {} >= ? {}", current_time - *ct.last_time.borrow(), ct.period );
                if current_time - *ct.last_time.borrow() > ct.period {
                    update_task_lasttime(msq, &ct, current_time);
                    msq.process_task(&crt, &ct);
                    if !ct.cyclicity {
                        // msq.queue_remove(&ct).await;
                        should_remove_tasks.push(ct);
                    }
                }
            }
            drop(mu);
            for rct in should_remove_tasks {
                msq.queue_remove(&rct).await;
            }
            match async_std::future::timeout(Duration::from_secs(1), notified.notified()).await {
                Ok(_) => {
                    log::warn!("Received a new task notification. The process loop continued.");
                }
                Err(err) => {
                    global_app_data_resizing(); // clear all data period
                    log::debug!("Time out for {}", err);
                }
            }
        }
    });
}

pub fn get_task_queue() -> &'static mut ProcessQueue {
    // 使用MaybeUninit延迟初始化
    static mut STATIC_ESQ: MaybeUninit<ProcessQueue> = MaybeUninit::uninit();
    // Once带锁保证只进行一次初始化
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        // CONF = 1u64;
        async_std::task::block_on(async {
            let esq = ProcessQueue::new();
            STATIC_ESQ.as_mut_ptr().write(esq);
        });
    });
    unsafe { &mut *STATIC_ESQ.as_mut_ptr() }
}

pub fn config_task_queue_wait_timeout(tmt: u64, _period: u64) {
    get_task_queue().waiting_timeout = tmt;
}

pub fn config_task_queue_max_tasks(max: usize) {
    get_task_queue().max_tasks = max;
}

pub fn start_task_queue_thread() {
    get_task_queue().start();
}

pub fn stop_task_queue_thread() {
    get_task_queue().shutdown();
}

pub fn notify_queue_process() {
    get_task_queue().notify();
}

pub fn count_queue_tasks() -> usize {
    get_task_queue().count()
}

pub fn task_queue_is_full() -> bool {
    get_task_queue().is_queue_full()
}

fn update_task_lasttime(msq: &mut ProcessQueue, ct: &ProcessTask, nsec: u64) {
    // let msq = get_task_queue();
    let len = msq.queue.len();

    for i in 0..len {
        if ct.task_id == msq.queue[i].task_id {
            *msq.queue[i].last_time.borrow_mut() = nsec;
        }
    }
}
