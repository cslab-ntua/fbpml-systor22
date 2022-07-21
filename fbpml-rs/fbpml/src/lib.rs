use std::fmt;
use std::time::Duration;

use anyhow::Result;
use tokio::time::Instant;

use fbpml_rpc::{
    one_argument_client::OneArgumentClient, two_arguments_client::TwoArgumentsClient,
    zero_arguments_client::ZeroArgumentsClient, ServiceResponse, TwoArgumentsRequest,
};

/// Represents the result of a single run of one of the rpc functions ([`zero_args_rpc`],
/// [`one_arg_rpc`] and [`two_args_rpc`]); thus includes results for one cold and one warm request.
pub struct Measurement {
    /// Global duration, measured for the "cold-start" request, also includes the delay for
    /// initializing the gRPC client and the gRPC request (i.e., in addition to the `cold`
    /// [`Delays`]), and possibly the delay required to restore the MicroVM from a snapshot and to
    /// resume it.
    pub global: Duration,
    /// The delay for restoring the MicroVM from a snapshot.
    restore: Duration,
    /// The delay for resuming the MicroVM after it has been restored from a snapshot.
    resume: Duration,
    /// The delays associated with the 'cold-start' request.
    cold: Delays,
    /// The delays associated with the 'warm' request.
    warm: Delays,
}

impl From<(Duration, Delays, Delays)> for Measurement {
    fn from((global, cold, warm): (Duration, Delays, Delays)) -> Self {
        Self {
            global,
            restore: Duration::ZERO,
            resume: Duration::ZERO,
            cold,
            warm,
        }
    }
}

impl From<(Duration, Duration, Duration, Delays, Delays)> for Measurement {
    fn from((g, rt, rm, cold, warm): (Duration, Duration, Duration, Delays, Delays)) -> Self {
        Self {
            global: g,
            restore: rt,
            resume: rm,
            cold,
            warm,
        }
    }
}

impl fmt::Display for Measurement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{}",
            self.global.as_micros(),
            self.restore.as_micros(),
            self.resume.as_micros(),
            self.cold.client.as_micros(),
            self.cold.server.as_micros(),
            self.warm.client.as_micros(),
            self.warm.server.as_micros()
        )
    }
}

/// Represents the result from issuing a single request (be it cold or warm) using one of the rpc
/// functions ([`zero_args_rpc`], [`one_arg_rpc`] and [`two_args_rpc`]).
pub struct Delays {
    /// The delay as measured by the client (i.e., it should include server's delay).
    client: Duration,
    /// The delay as measured by the server (i.e., it is sent over through the gRPC response).
    server: Duration,
}

impl From<(Duration, ServiceResponse)> for Delays {
    fn from((client, resp): (Duration, ServiceResponse)) -> Self {
        Self {
            client,
            server: resp
                .response_duration
                .unwrap_or_default()
                .try_into()
                .unwrap_or(Duration::ZERO),
        }
    }
}

pub async fn zero_args_rpc(server_addr: String) -> Result<(Duration, ServiceResponse)> {
    let mut client = ZeroArgumentsClient::connect(server_addr).await?;
    let req = tonic::Request::new(());

    // Issue the request & time it
    let client_start = Instant::now();
    let resp = client.bench(req).await?;
    let client_end = Instant::now();

    Ok((client_end - client_start, resp.into_inner()))
}

pub async fn one_arg_rpc(server_addr: String, arg: u64) -> Result<(Duration, ServiceResponse)> {
    let mut client = OneArgumentClient::connect(server_addr).await?;
    let req = tonic::Request::new(fbpml_rpc::OneArgumentRequest { arg });

    // Issue the request & time it
    let client_start = Instant::now();
    let resp = client.bench(req).await?;
    let client_end = Instant::now();

    Ok((client_end - client_start, resp.into_inner()))
}

pub async fn two_args_rpc(
    server_addr: String,
    arg1: u64,
    arg2: u64,
) -> Result<(Duration, ServiceResponse)> {
    let mut client = TwoArgumentsClient::connect(server_addr).await?;
    let req = tonic::Request::new(TwoArgumentsRequest { arg1, arg2 });

    // Issue the request & time it
    let client_start = Instant::now();
    let resp = client.bench(req).await?;
    let client_end = Instant::now();

    Ok((client_end - client_start, resp.into_inner()))
}
