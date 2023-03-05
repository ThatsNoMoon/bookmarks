use reqwest::{IntoUrl, Method, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use twilight_model::{
	application::command::Command,
	channel::{
		message::{Component, Embed, MessageFlags},
		Channel, Message,
	},
	id::{
		marker::{ChannelMarker, MessageMarker, UserMarker},
		Id,
	},
};
use worker::{Error, RouteContext};

use crate::{utils::Context as _, Result};

const API: &str = "https://discord.com/api/v10";

#[derive(Default, Serialize)]
pub(crate) struct CreateMessage {
	embeds: Option<Vec<Embed>>,
	components: Option<Vec<Component>>,
	flags: Option<MessageFlags>,
}

impl CreateMessage {
	pub(crate) fn embeds(self, embeds: impl Into<Vec<Embed>>) -> Self {
		Self {
			embeds: Some(embeds.into()),
			..self
		}
	}

	pub(crate) fn components(
		self,
		components: impl Into<Vec<Component>>,
	) -> Self {
		Self {
			components: Some(components.into()),
			..self
		}
	}

	pub(crate) fn flags(self, flags: MessageFlags) -> Self {
		Self {
			flags: Some(flags),
			..self
		}
	}
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
	code: u32,
	message: String,
}

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
			format!("{API}/applications/{}/commands", self.id),
			commands,
		)
		.await?;

		Ok(())
	}

	pub(crate) async fn send_dm(
		&mut self,
		recipient: Id<UserMarker>,
		message: &CreateMessage,
	) -> Result<Result<(Channel, Message), ()>> {
		let channel: Channel = self
			.send(
				Method::POST,
				format!("{API}/users/@me/channels"),
				json!({ "recipient_id": recipient }),
			)
			.await
			.context("Failed to create DM channel")?
			.json()
			.await
			.context("Failed to create DM channel")?;

		let res = self
			.send_ignore_status(
				Method::POST,
				format!("{API}/channels/{}/messages", channel.id),
				message,
			)
			.await?;

		let status = res.status();

		if status.is_server_error() {
			Err(Error::RustError(format!(
				"Internal Discord error {}",
				status
			)))
		} else if status.is_client_error() {
			match res.json::<ErrorResponse>().await {
				Ok(e) if e.code == 50007 => Ok(Err(())),
				Ok(e) => Err(Error::RustError(format!(
					"Failed to send DM: {}",
					e.message
				))),
				Err(_) => Err(Error::RustError(format!(
					"Unknown Discord error {}",
					status
				))),
			}
		} else {
			res.json::<Message>()
				.await
				.context("Invalid message received")
				.map(|msg| Ok((channel, msg)))
		}
	}

	pub(crate) async fn delete_message(
		&mut self,
		channel_id: Id<ChannelMarker>,
		message_id: Id<MessageMarker>,
	) -> Result<()> {
		self.send(
			Method::DELETE,
			format!("{API}/channels/{channel_id}/messages/{message_id}"),
			"",
		)
		.await?;

		Ok(())
	}

	async fn send(
		&mut self,
		method: Method,
		url: impl IntoUrl,
		body: impl Serialize,
	) -> Result<Response> {
		let res = self.send_ignore_status(method, url, body).await?;

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

	async fn send_ignore_status(
		&mut self,
		method: Method,
		url: impl IntoUrl,
		body: impl Serialize,
	) -> Result<Response> {
		self.inner
			.request(method, url)
			.header("Authorization", &self.token)
			.header("Content-Type", "application/json")
			.json(&body)
			.send()
			.await
			.map_err(|e| Error::RustError(e.to_string()))
	}
}
