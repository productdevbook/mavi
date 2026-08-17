use std::{fmt::Debug, future::Future, pin::Pin};

use crate::{Money, Result, SiteContext};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Clock: Debug + Send + Sync {
    fn now(&self) -> chrono::DateTime<chrono::Utc>;
}

pub trait FileStore: Debug + Send + Sync {
    fn put<'a>(
        &'a self,
        context: &'a SiteContext,
        path: &'a str,
        bytes: Vec<u8>,
    ) -> BoxFuture<'a, Result<()>>;
    fn get<'a>(&'a self, context: &'a SiteContext, path: &'a str)
    -> BoxFuture<'a, Result<Vec<u8>>>;
    fn remove<'a>(&'a self, context: &'a SiteContext, path: &'a str) -> BoxFuture<'a, Result<()>>;
}

pub trait Mailer: Debug + Send + Sync {
    fn send<'a>(
        &'a self,
        context: &'a SiteContext,
        message: MailMessage,
    ) -> BoxFuture<'a, Result<()>>;
}

#[derive(Clone, Debug)]
pub struct MailMessage {
    pub recipient: String,
    pub subject: String,
    pub body: String,
}

pub trait Payments: Debug + Send + Sync {
    fn charge<'a>(
        &'a self,
        context: &'a SiteContext,
        amount: Money,
    ) -> BoxFuture<'a, Result<PaymentReceipt>>;
}

#[derive(Clone, Debug)]
pub struct PaymentReceipt {
    pub provider: String,
    pub reference: String,
}

pub trait Builds: Debug + Send + Sync {
    fn build<'a>(
        &'a self,
        context: &'a SiteContext,
        source: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
}

pub trait Seals: Debug + Send + Sync {
    fn seal<'a>(
        &'a self,
        context: &'a SiteContext,
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
    fn unseal<'a>(
        &'a self,
        context: &'a SiteContext,
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>>;
}
