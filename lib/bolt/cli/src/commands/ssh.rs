use anyhow::*;
use clap::{Parser, ValueEnum};

use bolt_core::{context::ProjectContext, tasks::ssh::TempSshKey};

#[derive(ValueEnum, Clone)]
pub enum Format {
	Json,
}

#[derive(Parser)]
pub enum SubCommand {
	Ip {
		#[clap(index = 1)]
		ip: String,
		#[clap(index = 2)]
		command: Option<String>,
		#[clap(long)]
		ssh_key: Option<String>,
	},
	Name {
		#[clap(index = 1)]
		name: String,
		#[clap(index = 2)]
		command: Option<String>,
	},
	Pool {
		#[clap(index = 1)]
		pool: String,
		#[clap(index = 2)]
		command: Option<String>,
	},
}

impl SubCommand {
	pub async fn execute(self, ctx: ProjectContext) -> Result<()> {
		match self {
			Self::Ip {
				ip,
				command,
				ssh_key,
			} => {
				let ssh_key = TempSshKey::new(
					&ctx,
					&ssh_key.map_or_else(|| "salt_minion".to_string(), |x| x),
				)
				.await?;
				bolt_core::tasks::ssh::ip(
					&ctx,
					&ip,
					&ssh_key,
					command.as_ref().map(String::as_str),
				)
				.await?;
			}
			Self::Name { name, command } => {
				bolt_core::tasks::ssh::name(&ctx, &name, command.as_ref().map(String::as_str))
					.await?;
			}
			Self::Pool { pool, command } => {
				bolt_core::tasks::ssh::pool(&ctx, &pool, command.as_ref().map(String::as_str))
					.await?;
			}
		}

		Ok(())
	}
}
