use r2d2::Pool;
use redis::{
    cluster::ClusterClientBuilder, from_redis_value, ConnectionLike, FromRedisValue, RedisError,
    RedisResult,
};
use std::time::Duration;

use crate::AppConfig;
use crate::ChimesError;
use crate::InstanceType;
use crate::RedisConfig;

impl RedisConfig {
    pub fn to_redis_client(&self) -> RedisResult<RedisClient> {
        match self.instance_type.clone() {
            None => Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Config Error",
            ))),
            Some(t) => match t {
                InstanceType::Single => {
                    let cl = redis::Client::open(self.urls[0].as_str())?;
                    Ok(RedisClient::Single(cl))
                }
                InstanceType::Cluster => {
                    let mut cb = ClusterClientBuilder::new(self.urls.clone());
                    if self.password.is_some() {
                        cb = cb.password(self.password.clone().unwrap());
                    }
                    let cl = cb.build()?;
                    Ok(RedisClient::Cluster(cl))
                }
            },
        }
    }
}

#[derive(Clone)]
pub enum RedisClient {
    Single(redis::Client),
    Cluster(redis::cluster::ClusterClient),
}

impl RedisClient {
    pub fn get_redis_connection(&self) -> RedisResult<RedisConnection> {
        match self {
            RedisClient::Single(s) => {
                let conn = s.get_connection()?;
                Ok(RedisConnection::Single(Box::new(conn)))
            }
            RedisClient::Cluster(c) => {
                let conn = c.get_connection()?;
                Ok(RedisConnection::Cluster(Box::new(conn)))
            }
        }
    }
}

pub enum RedisConnection {
    Single(Box<redis::Connection>),
    Cluster(Box<redis::cluster::ClusterConnection>),
}

impl RedisConnection {
    pub fn is_open(&self) -> bool {
        match self {
            RedisConnection::Single(sc) => sc.is_open(),
            RedisConnection::Cluster(cc) => cc.is_open(),
        }
    }

    pub fn query<T: FromRedisValue>(&mut self, cmd: &redis::Cmd) -> RedisResult<T> {
        match self {
            RedisConnection::Single(sc) => match sc.as_mut().req_command(cmd) {
                Ok(val) => from_redis_value(&val),
                Err(e) => Err(e),
            },
            RedisConnection::Cluster(cc) => match cc.req_command(cmd) {
                Ok(val) => from_redis_value(&val),
                Err(e) => Err(e),
            },
        }
    }
}

#[derive(Clone)]
pub struct RedisConnectionManager {
    pub redis_client: RedisClient,
}

// ToDo 实现 broken 函数
impl r2d2::ManageConnection for RedisConnectionManager {
    type Connection = RedisConnection;
    type Error = RedisError;

    fn connect(&self) -> Result<RedisConnection, Self::Error> {
        let conn = self.redis_client.get_redis_connection()?;
        Ok(conn)
    }

    fn is_valid(&self, conn: &mut RedisConnection) -> Result<(), Self::Error> {
        match conn {
            RedisConnection::Single(sc) => {
                redis::cmd("PING").query(sc)?;
            }
            RedisConnection::Cluster(cc) => {
                redis::cmd("PING").query(cc)?;
            }
        }
        Ok(())
    }

    fn has_broken(&self, conn: &mut RedisConnection) -> bool {
        !conn.is_open()
    }
}

// pub fn init_redis_pool() {
//     GLOBAL_REDIS_POOL.get_or_init(|| {
//         let pool = gen_redis_conn_pool().unwrap();
//         pool
//     });
// }

pub fn gen_redis_conn_pool() -> Result<Pool<RedisConnectionManager>, ChimesError> {
    let appconf = AppConfig::get().lock().unwrap().to_owned();
    if appconf.redis_conf.is_none() {
        Err(ChimesError::custom(10070, "Config Error".to_string()))
    } else {
        let redis_conf = appconf.redis_conf.unwrap();
        let redis_client: RedisClient = match redis_conf.to_redis_client() {
            Ok(rc) => rc,
            Err(err) => {
                return Err(ChimesError::custom(10071, err.to_string()));
            }
        };
        let manager = RedisConnectionManager { redis_client };
        match r2d2::Pool::builder()
            .max_size(redis_conf.pool.max_size)
            .min_idle(Some(redis_conf.pool.mini_idel))
            .connection_timeout(Duration::from_secs(redis_conf.pool.connection_timeout))
            .build(manager)
        {
            Ok(pool) => Ok(pool),
            Err(err) => Err(ChimesError::custom(10071, err.to_string())),
        }
    }
}

use once_cell::sync::OnceCell;

pub static GLOBAL_REDIS_POOL: OnceCell<Option<r2d2::Pool<RedisConnectionManager>>> =
    OnceCell::new();

pub fn init_global_redis() {
    GLOBAL_REDIS_POOL.get_or_init(|| match gen_redis_conn_pool() {
        Ok(it) => Some(it),
        Err(err) => {
            log::error!("Error for init the Redis connection: {}", err);
            None
        }
    });
}

pub fn get_redis_connection() -> Option<Pool<RedisConnectionManager>> {
    let conn: Option<&Option<Pool<RedisConnectionManager>>> = GLOBAL_REDIS_POOL.get();
    if let Some(conn) = conn {
        conn.to_owned()
    } else {
        None
    }
}

pub fn redis_get(key: &str) -> Result<Option<String>, ChimesError> {
    let conn = get_redis_connection();
    match conn {
        Some(c) => match c.get() {
            Ok(mut tc) => match tc.query::<String>(redis::cmd("GET").arg(key)) {
                Ok(xv) => Ok(Some(xv)),
                Err(err) => Err(ChimesError::custom(10070, err.to_string())),
            },
            Err(err) => Err(ChimesError::custom(10070, err.to_string())),
        },
        None => Ok(None),
    }
}

pub fn redis_set(key: &str, value: &str) -> Result<Option<String>, ChimesError> {
    let conn = get_redis_connection();
    match conn {
        Some(c) => match c.get() {
            Ok(mut tc) => match tc.query::<String>(redis::cmd("SET").arg(key).arg(value)) {
                Ok(xv) => Ok(Some(xv)),
                Err(err) => Err(ChimesError::custom(10076, err.to_string())),
            },
            Err(err) => Err(ChimesError::custom(10076, err.to_string())),
        },
        None => Ok(None),
    }
}

pub fn redis_set_expire(
    key: &str,
    value: &str,
    expire: u64,
) -> Result<Option<String>, ChimesError> {
    let conn = get_redis_connection();
    match conn {
        Some(c) => match c.get() {
            Ok(mut tc) => {
                match tc
                    .query::<String>(redis::cmd("SET").arg(key).arg(value).arg("EX").arg(expire))
                {
                    Ok(xv) => Ok(Some(xv)),
                    Err(err) => Err(ChimesError::custom(10077, err.to_string())),
                }
            }
            Err(err) => Err(ChimesError::custom(10077, err.to_string())),
        },
        None => Ok(None),
    }
}

pub fn redis_del(key: &str) -> Result<Option<String>, ChimesError> {
    let conn = get_redis_connection();
    match conn {
        Some(c) => match c.get() {
            Ok(mut tc) => match tc.query::<String>(redis::cmd("DEL").arg(key)) {
                Ok(xv) => Ok(Some(xv)),
                Err(err) => Err(ChimesError::custom(10077, err.to_string())),
            },
            Err(err) => Err(ChimesError::custom(10077, err.to_string())),
        },
        None => Ok(None),
    }
}
