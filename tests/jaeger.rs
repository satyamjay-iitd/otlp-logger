use testcontainers::{core::WaitFor, Image};

const NAME: &str = "jaegertracing/all-in-one";
const TAG: &str = "1.56";
const DEFAULT_WAIT: u64 = 3000;

#[derive(Debug, Default, Clone)]
pub struct Jaeger;


impl Image for Jaeger {
    type Args = ();

    fn name(&self) -> String {
        NAME.to_owned()
    }

    fn tag(&self) -> String {
        TAG.to_owned()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![
            WaitFor::message_on_stderr("Channel Connectivity change to READY"),
            WaitFor::millis(DEFAULT_WAIT),
        ]
    }
}