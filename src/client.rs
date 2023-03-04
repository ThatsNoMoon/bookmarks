use reqwest::{IntoUrl, Method, Response};
use serde::Serialize;
use twilight_model::{
	application::command::Command,
	id::{marker::GuildMarker, Id},
};
use worker::Error;

use crate::utils::Context as _;

const URL_BASE: &str = "https://discord.com/api/v10";

pub(crate) struct Client {
	inner: reqwest::Client,
	token: String,
	id: u64,
}

impl Client {
	pub(crate) fn new(token: String, id: u64) -> Self {
		Self {
			inner: reqwest::Client::new(),
			token: format!("Bot {token}"),
			id,
		}
	}

	pub(crate) async fn create_commands(
		&mut self,
		commands: Vec<Command>,
		guild_id: Option<Id<GuildMarker>>,
	) -> Result<(), Error> {
		let url = match guild_id {
			Some(id) => format!(
				"{URL_BASE}/applications/{}/guilds/{id}/commands",
				self.id
			),
			None => format!("{URL_BASE}/applications/{}/commands", self.id),
		};

		let res = self.send(Method::PUT, url, commands).await?;

		if res.status().is_client_error() {
			return Err(Error::RustError(
				res.text()
					.await
					.context("Failed to get error from Discord")?,
			));
		}

		res.error_for_status().context("Discord error")?;

		Ok(())
	}

	async fn send(
		&mut self,
		method: Method,
		url: impl IntoUrl,
		body: impl Serialize,
	) -> Result<Response, Error> {
		self.inner
			.request(method, url)
			.header("Authorization", &self.token)
			.header("Content-Type", "application/json")
			.body(serde_json::to_vec(&body)?)
			.send()
			.await
			.map_err(|e| Error::RustError(e.to_string()))
	}
}
