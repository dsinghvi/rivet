use api_helper::{
	auth::{ApiAuth, AuthRateLimitCtx},
	util::{as_auth_expired, basic_rate_limit},
};
use proto::claims::Claims;
use rivet_claims::ClaimsDecode;
use rivet_operation::prelude::*;

/// Information derived from the authentication middleware.
pub struct Auth {
	claims: Option<Claims>,
}

#[async_trait]
impl ApiAuth for Auth {
	async fn new(
		api_token: Option<String>,
		rate_limit_ctx: AuthRateLimitCtx<'_>,
	) -> GlobalResult<Auth> {
		Self::rate_limit(rate_limit_ctx).await?;

		Ok(Auth {
			claims: if let Some(api_token) = api_token {
				Some(as_auth_expired(rivet_claims::decode(&api_token)?)?)
			} else {
				None
			},
		})
	}

	async fn rate_limit(rate_limit_ctx: AuthRateLimitCtx<'_>) -> GlobalResult<()> {
		basic_rate_limit(rate_limit_ctx).await
	}
}

impl Auth {
	pub fn claims(&self) -> GlobalResult<&Claims> {
		self.claims
			.as_ref()
			.ok_or_else(|| err_code!(API_UNAUTHORIZED, reason = "No bearer token provided."))
	}

	pub async fn user(&self, ctx: &OperationContext<()>) -> GlobalResult<rivet_claims::ent::User> {
		let claims = self.claims()?;
		let user_ent = claims.as_user()?;

		let user_res = op!([ctx] user_get {
			user_ids: vec![user_ent.user_id.into()],
		})
		.await?;
		let user = internal_unwrap_owned!(user_res.users.first());

		// Verify user is not deleted
		if user.delete_complete_ts.is_some() {
			let jti = internal_unwrap_owned!(claims.jti);
			op!([ctx] token_revoke {
				jtis: vec![jti],
			})
			.await?;

			panic_with!(TOKEN_REVOKED);
		}

		Ok(user_ent)
	}
}
