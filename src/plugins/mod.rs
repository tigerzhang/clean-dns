use anyhow::Result;
use async_trait::async_trait;
use hickory_proto::op::Message;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

pub mod cache;
pub mod delay_plugin;
pub mod domain_set;
pub mod forward;
pub mod hosts;
pub mod if_plugin;
pub mod ip_set;
pub mod matcher;
pub mod reject_plugin;
pub mod return_plugin;
pub mod sequence;

/// Context holds the state of a DNS query handling.
#[derive(Clone)]
pub struct Context {
    pub client_addr: SocketAddr,
    pub request: Message,
    pub response: Option<Message>,
    pub abort: bool,
}

impl Context {
    pub fn new(client_addr: SocketAddr, request: Message) -> Self {
        Self {
            client_addr,
            request,
            response: None,
            abort: false,
        }
    }
}

pub trait DomainSet: Send + Sync {
    fn contains(&self, domain: &str) -> bool;
}

pub trait IpSet: Send + Sync {
    fn contains(&self, ip: IpAddr) -> bool;
}

pub trait Condition: Send + Sync {
    fn check(&self, ctx: &Context) -> bool;
}

/// Plugin is the core interface for processing DNS requests.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Name of the plugin.
    fn name(&self) -> &str;

    /// Execute the plugin logic.
    async fn next(&self, ctx: &mut Context) -> Result<()>;

    // Data provider interface
    fn as_domain_set(&self) -> Option<&dyn DomainSet> {
        None
    }

    fn as_ip_set(&self) -> Option<&dyn IpSet> {
        None
    }

    fn as_condition(&self) -> Option<&dyn Condition> {
        None
    }
}

/// Type alias for a shared plugin instance.
pub type SharedPlugin = Arc<dyn Plugin>;
