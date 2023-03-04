use reqwest::{IntoUrl, Method, Response};
use serde::Serialize;
use serde_json::json;
use twilight_model::{
	application::command::Command,
	channel::{Channel, Message},
	id::{marker::UserMarker, Id},
};
use worker::{Error, Result, RouteContext};

use crate::utils::Context as _;

const URL_BASE: &str = "https://discord.com/api/v10";

pub(crate) struct Client {
	inner: reqwest::Client,
	token: String,
	id: u64,
}

impl Client {
	pub(crate) fn new<D>(ctx: RouteContext<D>) -> Result<Self> {
		let id = ctx
			.var("DISCORD_APPLICATION_ID")?
			.to_string()
			.parse()
			.context("Failed to parse application ID")?;
		let token = ctx.var("DISCORD_TOKEN")?.to_string();

		Ok(Self {
			inner: reqwest::Client::new(),
			token: format!("Bot {token}"),
			id,
		})
	}

	pub(crate) async fn create_commands(
		&mut self,
		commands: Vec<Command>,
	) -> Result<()> {
		self.send(
			Method::PUT,
			format!("{URL_BASE}/applications/{}/commands", self.id),
			commands,
		)
		.await?;

		Ok(())
	}

	pub(crate) async fn send_dm(
		&mut self,
		recipient: Id<UserMarker>,
		message: &Message,
	) -> Result<()> {
		let channel: Channel = self
			.send(
				Method::POST,
				format!("{URL_BASE}/users/@me/channels"),
				json!({ "recipient_id": recipient }),
			)
			.await
			.context("Failed to create DM channel")?
			.json()
			.await
			.context("Failed to create DM channel")?;

		self.send(
			Method::POST,
			format!("{URL_BASE}/channels/{}/messages", channel.id),
			message,
		)
		.await
		.context("Failed to send DM")?;

		Ok(())
	}

	async fn send(
		&mut self,
		method: Method,
		url: impl IntoUrl,
		body: impl Serialize,
	) -> Result<Response> {
		let res = self
			.inner
			.request(method, url)
			.header("Authorization", &self.token)
			.header("Content-Type", "application/json")
			.json(&body)
			.send()
			.await
			.map_err(|e| Error::RustError(e.to_string()))?;

		if res.status().is_client_error() {
			let msg = res
				.text()
				.await
				.context("Failed to get error from Discord")?;
			Err(Error::RustError(format!("Error from Discord: {msg}")))
		} else {
			res.error_for_status().context("Discord error")
		}
	}
}
