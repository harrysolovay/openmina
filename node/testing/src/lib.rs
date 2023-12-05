mod exit_with_error;
pub use exit_with_error::exit_with_error;

pub mod cluster;
pub mod node;
pub mod scenario;
#[cfg(feature = "scenario-generators")]
pub mod scenarios;
pub mod service;

pub mod network_debugger;
pub mod ocaml;

mod server;
pub use server::server;

pub fn setup() -> tokio::runtime::Runtime {
    // openmina_node_native::tracing::initialize(openmina_node_native::tracing::Level::DEBUG);
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus::get().max(2) - 1)
        .thread_name(|i| format!("openmina_rayon_{i}"))
        .build_global()
        .unwrap();

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

pub fn setup_without_rt() {
    lazy_static::lazy_static! {
        static ref INIT: () = {
            openmina_node_native::tracing::initialize(openmina_node_native::tracing::Level::WARN);
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_cpus::get().max(2) - 1)
                .thread_name(|i| format!("openmina_rayon_{i}"))
                .build_global()
                .unwrap();
        };
    };
    *INIT;
}
