use api_helper::ctx::Ctx;
use proto::backend::{self, pkg::*};
use redis::AsyncCommands;
use rivet_operation::prelude::*;

use crate::{auth::Auth, route::traefik};

#[tracing::instrument(skip(ctx))]
pub async fn build(
	ctx: &Ctx<Auth>,
	pool: &str,
	region: &str,
) -> GlobalResult<traefik::TraefikConfigResponse> {
	let mut config = traefik::TraefikConfigResponse::default();

	if pool != "ing-job" {
		return Ok(config);
	}

	// TODO: Cache this
	// Determine the region from the query
	let region_resolve_res = op!([ctx] region_resolve {
		name_ids: vec![region.to_string()],
	})
	.await?;
	let region_id =
		internal_unwrap!(internal_unwrap!(region_resolve_res.regions.first()).region_id);

	let redis_job = ctx.op_ctx().redis_job().await?;
	let job_runs_fetch = fetch_job_runs(redis_job, region_id.as_uuid()).await?;

	config.http.middlewares.insert(
		"job-rate-limit".to_owned(),
		traefik::TraefikMiddleware::RateLimit {
			average: 100,
			period: "5m".into(),
			burst: 256,
			source_criterion: traefik::InFlightReqSourceCriterion::IpStrategy(
				traefik::IpStrategy {
					depth: 0,
					exclude_ips: None,
				},
			),
		},
	);
	config.http.middlewares.insert(
		"job-in-flight".to_owned(),
		traefik::TraefikMiddleware::InFlightReq {
			// This number needs to be high to allow for parallel requests
			amount: 4,
			source_criterion: traefik::InFlightReqSourceCriterion::IpStrategy(
				traefik::IpStrategy {
					depth: 0,
					exclude_ips: None,
				},
			),
		},
	);

	// Process proxied ports
	for run_proxied_ports in &job_runs_fetch {
		let run_id = internal_unwrap!(run_proxied_ports.run_id);
		tracing::info!(proxied_ports_len = ?run_proxied_ports.proxied_ports.len(), "adding job run");
		for proxied_port in &run_proxied_ports.proxied_ports {
			let register_res = register_proxied_port(**run_id, proxied_port, &mut config);
			match register_res {
				Ok(_) => {}
				Err(err) => {
					tracing::error!(?err, ?proxied_port, "failed to register proxied port route")
				}
			}
		}
	}

	tracing::info!(
		http_services = ?config.http.services.len(),
		http_routers = ?config.http.routers.len(),
		http_middlewares = ?config.http.middlewares.len(),
		tcp_services = ?config.tcp.services.len(),
		tcp_routers = ?config.tcp.routers.len(),
		tcp_middlewares = ?config.tcp.middlewares.len(),
		udp_services = ?config.udp.services.len(),
		udp_routers = ?config.udp.routers.len(),
		udp_middlewares = ?config.udp.middlewares.len(),
		"job traefik config"
	);

	Ok(config)
}

#[tracing::instrument(skip(redis_job))]
async fn fetch_job_runs(
	mut redis_job: RedisPool,
	region_id: Uuid,
) -> GlobalResult<Vec<job::redis_job::RunProxiedPorts>> {
	let runs = redis_job
		.hvals::<_, Vec<Vec<u8>>>(util_job::key::proxied_ports(region_id))
		.await?
		.into_iter()
		.filter_map(
			|buf| match job::redis_job::RunProxiedPorts::decode(buf.as_slice()) {
				Ok(x) => Some(x),
				Err(err) => {
					tracing::error!(?err, "failed to decode run RunProxiedPorts from redis");
					None
				}
			},
		)
		.collect::<Vec<_>>();
	let proxied_port_len = runs.iter().fold(0, |acc, x| acc + x.proxied_ports.len());
	tracing::info!(runs_len = ?runs.len(), ?proxied_port_len, "fetched job runs");
	Ok(runs)
}

#[tracing::instrument(skip(config))]
fn register_proxied_port(
	run_id: Uuid,
	proxied_port: &job::redis_job::run_proxied_ports::ProxiedPort,
	config: &mut traefik::TraefikConfigResponse,
) -> GlobalResult<()> {
	use backend::job::ProxyProtocol;

	let ingress_port = proxied_port.ingress_port;
	let target_nomad_port_label = internal_unwrap!(proxied_port.target_nomad_port_label);
	let service_id = format!("job-run:{}:{}", run_id, target_nomad_port_label);
	let proxy_protocol = internal_unwrap_owned!(backend::job::ProxyProtocol::from_i32(
		proxied_port.proxy_protocol
	));

	// Insert the relevant service
	match proxy_protocol {
		ProxyProtocol::Http | ProxyProtocol::Https => {
			config.http.services.insert(
				service_id.clone(),
				traefik::TraefikService {
					load_balancer: traefik::TraefikLoadBalancer {
						servers: vec![traefik::TraefikServer {
							url: Some(format!(
								"http://{}:{}",
								proxied_port.ip, proxied_port.source
							)),
							address: None,
						}],
						sticky: None,
					},
				},
			);
		}
		ProxyProtocol::Tcp | ProxyProtocol::TcpTls => {
			config.tcp.services.insert(
				service_id.clone(),
				traefik::TraefikService {
					load_balancer: traefik::TraefikLoadBalancer {
						servers: vec![traefik::TraefikServer {
							url: None,
							address: Some(format!("{}:{}", proxied_port.ip, proxied_port.source)),
						}],
						sticky: None,
					},
				},
			);
		}
		ProxyProtocol::Udp => {
			config.udp.services.insert(
				service_id.clone(),
				traefik::TraefikService {
					load_balancer: traefik::TraefikLoadBalancer {
						servers: vec![traefik::TraefikServer {
							url: None,
							address: Some(format!("{}:{}", proxied_port.ip, proxied_port.source)),
						}],
						sticky: None,
					},
				},
			);
		}
	};

	// Insert the relevant router
	match proxy_protocol {
		ProxyProtocol::Http => {
			config.http.routers.insert(
				format!("job-run:{}:{}:http", run_id, target_nomad_port_label),
				traefik::TraefikRouter {
					entry_points: vec![format!("lb-{ingress_port}")],
					rule: Some(format_http_rule(proxied_port)),
					priority: None,
					service: service_id,
					middlewares: vec!["job-rate-limit".into(), "job-in-flight".into()],
					tls: None,
				},
			);
		}
		ProxyProtocol::Https => {
			config.http.routers.insert(
				format!("job-run:{}:{}:https", run_id, target_nomad_port_label),
				traefik::TraefikRouter {
					entry_points: vec![format!("lb-{ingress_port}")],
					rule: Some(format_http_rule(proxied_port)),
					priority: None,
					service: service_id,
					middlewares: vec!["job-rate-limit".into(), "job-in-flight".into()],
					tls: Some(traefik::TraefikTls::build(build_tls_domains(proxied_port)?)),
				},
			);
		}
		ProxyProtocol::Tcp => {
			config.tcp.routers.insert(
				format!("job-run:{}:{}:tcp", run_id, target_nomad_port_label),
				traefik::TraefikRouter {
					entry_points: vec![format!("lb-{ingress_port}")],
					rule: Some("HostSNI(`*`)".into()),
					priority: None,
					service: service_id,
					middlewares: vec![],
					tls: None,
				},
			);
		}
		ProxyProtocol::TcpTls => {
			config.tcp.routers.insert(
				format!("job-run:{}:{}:tcp-tls", run_id, target_nomad_port_label),
				traefik::TraefikRouter {
					entry_points: vec![format!("lb-{ingress_port}")],
					rule: Some("HostSNI(`*`)".into()),
					priority: None,
					service: service_id,
					middlewares: vec![],
					tls: Some(traefik::TraefikTls::build(build_tls_domains(proxied_port)?)),
				},
			);
		}
		ProxyProtocol::Udp => {
			config.udp.routers.insert(
				format!("job-run:{}:{}:udp", run_id, target_nomad_port_label),
				traefik::TraefikRouter {
					entry_points: vec![format!("lb-{ingress_port}")],
					rule: None,
					priority: None,
					service: service_id,
					middlewares: vec![],
					tls: None,
				},
			);
		}
	}

	Ok(())
}

fn format_http_rule(proxied_port: &job::redis_job::run_proxied_ports::ProxiedPort) -> String {
	proxied_port
		.ingress_hostnames
		.iter()
		.map(|x| format!("Host(`{}`)", x))
		.collect::<Vec<String>>()
		.join(" || ")
}

fn build_tls_domains(
	proxied_port: &job::redis_job::run_proxied_ports::ProxiedPort,
) -> GlobalResult<Vec<traefik::TraefikTlsDomain>> {
	// Derive TLS config. Jobs can specify their own ingress rules, so we
	// need to derive which domains to use for the job.
	//
	// An exact SSL mode will only work with one specific domain. This is
	// very rarely used.
	//
	// A parent wildcard SSL mode will use the parent domain as the SSL
	// name.
	let ssl_domain_mode = internal_unwrap_owned!(backend::job::SslDomainMode::from_i32(
		proxied_port.ssl_domain_mode,
	));
	let mut domains = Vec::new();
	match ssl_domain_mode {
		backend::job::SslDomainMode::Exact => {
			for host in &proxied_port.ingress_hostnames {
				domains.push(traefik::TraefikTlsDomain {
					main: host.clone(),
					sans: Vec::new(),
				});
			}
		}
		backend::job::SslDomainMode::ParentWildcard => {
			for host in &proxied_port.ingress_hostnames {
				let (_, parent_host) = internal_unwrap_owned!(host.split_once('.'));
				domains.push(traefik::TraefikTlsDomain {
					main: parent_host.to_owned(),
					sans: vec![format!("*.{}", parent_host)],
				});
			}
		}
	}

	Ok(domains)
}
