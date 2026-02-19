mod app;
mod config;
mod http;
mod outline;
mod state_store;
mod types;

fn main() {
	let runtime = tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.worker_threads(config::runtime_workers())
		.max_blocking_threads(2)
		.build()
		.expect("failed to build Tokio runtime");

	if let Err(err) = runtime.block_on(app::app_main()) {
		eprintln!("fatal error: {err:#}");
		std::process::exit(1);
	}
}
