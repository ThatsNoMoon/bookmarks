use client::CreateMessage;
use ed25519_dalek::{ed25519::signature::Signature, PublicKey, Verifier};
use twilight_model::{
	application::{
		command::CommandType,
		interaction::{
			application_command::CommandData, Interaction, InteractionData,
			InteractionType,
		},
	},
	channel::message::{
		component::{ActionRow, Button, ButtonStyle},
		Component, MessageFlags, ReactionType,
	},
	http::interaction::{InteractionResponse, InteractionResponseType},
	id::{marker::GuildMarker, Id},
	user::User,
	util::Timestamp,
};
use twilight_util::{
	builder::{
		command::CommandBuilder,
		embed::{EmbedAuthorBuilder, EmbedBuilder, ImageSource},
		InteractionResponseDataBuilder,
	},
	snowflake::Snowflake,
};
use worker::*;

use crate::{client::Client, utils::Context as _};

mod client;
mod utils;

const COMMAND_NAME: &str = "Bookmark message";

const CDN: &str = "https://cdn.discordapp.com";

const DISCORD: &str = "https://discord.com";

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
	utils::set_panic_hook();

	let router = Router::new();

	router
		.post_async("/", |mut req, ctx| async move {
			let body = req.text().await?;

			if let Err(e) = check_signature(&req, &body, &ctx)? {
				console_log!("{e}");
				return Response::error("Signature verification failed", 401);
			}

			let interaction: Interaction = serde_json::from_str(&body)?;

			match interaction.kind {
				InteractionType::Ping => {
					let resp = InteractionResponse {
						kind: InteractionResponseType::Pong,
						data: None,
					};
					console_log!("Ping received");
					Response::from_json(&resp)
				}
				InteractionType::ApplicationCommand => {
					let data = match interaction.data {
						Some(InteractionData::ApplicationCommand(data)) => data,
						_ => {
							return Response::error(
								"Unexpected interaction data type",
								400,
							)
						}
					};

					console_log!(r#"Command "{}" received"#, data.name);

					let user = interaction
						.member
						.context("No member provided")?
						.user
						.context("Member contained no user")?;

					let guild_id = interaction
						.guild_id
						.context("Command not used in guild")?;

					bookmark(*data, guild_id, user, ctx).await
				}
				InteractionType::MessageComponent => {
					let data = match interaction.data {
						Some(InteractionData::MessageComponent(data)) => data,
						_ => {
							return Response::error(
								"Unexpected interaction data type",
								400,
							)
						}
					};

					console_log!(
						r#"Message component "{}" received"#,
						data.custom_id
					);

					if data.custom_id == "delete" {
						let channel_id = interaction
							.channel_id
							.context("Delete button had no channel")?;
						let message_id = interaction
							.message
							.context("Delete button had no message")?
							.id;

						Client::new(ctx)?
							.delete_message(channel_id, message_id)
							.await?;

						Response::empty()
					} else {
						Response::error("Unexpected custom_id", 400)
					}
				}
				_ => Response::error("Unexpected interaction type", 400),
			}
		})
		.post_async("/register", |_, ctx| async move {
			console_log!("Registering commands");
			register_bookmark_command(ctx).await
		})
		.run(req, env)
		.await
}

async fn bookmark<D>(
	command: CommandData,
	guild_id: Id<GuildMarker>,
	user: User,
	ctx: RouteContext<D>,
) -> Result<Response> {
	match &*command.name {
		COMMAND_NAME => (),
		c => return Err(Error::RustError(format!("Unknown command {c}"))),
	}

	match command.kind {
		CommandType::Message => (),
		_ => {
			return Err(Error::RustError("Unexpected command type".to_owned()))
		}
	}

	let target = command.target_id.context("No target ID")?;

	let resolved = command.resolved.context("No resolved data")?;

	let message = resolved
		.messages
		.get(&target.cast())
		.context("No resolved message")?;

	let message_link = format!(
		"{DISCORD}/channels/{}/{}/{}",
		guild_id, message.channel_id, message.id
	);

	let author = {
		let avatar_url = match message.author.avatar {
			Some(hash) => {
				format!("{CDN}/avatars/{}/{}.webp", message.author.id, hash)
			}

			None => format!(
				"{CDN}/embed/avatars/{}.png",
				message.author.discriminator % 5
			),
		};

		let image = ImageSource::url(avatar_url)
			.context("Failed to create avatar image")?;

		EmbedAuthorBuilder::new(&message.author.name)
			.icon_url(image)
			.url(&message_link)
	};

	let embed = EmbedBuilder::new()
		.author(author)
		.description(&message.content)
		.timestamp(
			Timestamp::from_secs(message.id.timestamp() / 1000 as i64)
				.context("Failed to create timestamp")?,
		)
		.build();

	let bookmark = CreateMessage::default()
		.embeds([embed])
		.components([Component::ActionRow(ActionRow {
			components: vec![
				Component::Button(Button {
					url: Some(message_link),
					label: Some("Visit".to_owned()),
					style: ButtonStyle::Link,
					disabled: false,
					emoji: None,
					custom_id: None,
				}),
				Component::Button(Button {
					custom_id: Some("delete".to_owned()),
					emoji: Some(ReactionType::Unicode {
						name: "🗑".to_owned(),
					}),
					label: Some("Delete".to_owned()),
					style: ButtonStyle::Danger,
					disabled: false,
					url: None,
				}),
			],
		})])
		.flags(MessageFlags::SUPPRESS_NOTIFICATIONS);

	let (dm_channel, sent_msg) =
		Client::new(ctx)?.send_dm(user.id, &bookmark).await?;

	let sent_link =
		format!("{DISCORD}/channels/@me/{}/{}", dm_channel.id, sent_msg.id);

	let response = InteractionResponse {
		kind: InteractionResponseType::ChannelMessageWithSource,
		data: Some(
			InteractionResponseDataBuilder::new()
				.flags(MessageFlags::EPHEMERAL)
				.content("🔖 Message bookmarked")
				.components([Component::ActionRow(ActionRow {
					components: vec![Component::Button(Button {
						url: Some(sent_link),
						label: Some("Visit".to_owned()),
						style: ButtonStyle::Link,
						disabled: false,
						emoji: None,
						custom_id: None,
					})],
				})])
				.build(),
		),
	};

	Response::from_json(&response)
}

fn check_signature<D>(
	req: &Request,
	body: &str,
	ctx: &RouteContext<D>,
) -> Result<Result<()>> {
	let public_key = ctx.var("DISCORD_PUBLIC_KEY")?.to_string();
	let public_key = hex::decode(public_key).context("Non-hex public key")?;
	let public_key =
		PublicKey::from_bytes(&public_key).context("Invalid public key")?;
	Ok(verify_signature(req, body, public_key))
}

fn verify_signature(
	req: &Request,
	body: &str,
	public_key: PublicKey,
) -> Result<()> {
	let timestamp = req
		.headers()
		.get("X-Signature-Timestamp")?
		.context("No timestamp")?;
	let signed = format!("{}{}", timestamp, body).into_bytes();

	let signature = req
		.headers()
		.get("X-Signature-Ed25519")?
		.context("No signature")?;
	let signature = hex::decode(signature).context("Non-hex signature")?;
	let signature =
		Signature::from_bytes(&signature).context("Invalid signature")?;

	public_key
		.verify(&signed, &signature)
		.context("Signature verification failed")?;

	Ok(())
}

async fn register_bookmark_command<D>(
	ctx: RouteContext<D>,
) -> Result<Response> {
	let bookmark = CommandBuilder::new(COMMAND_NAME, "", CommandType::Message)
		.dm_permission(false)
		.build();

	if let Err(e) = Client::new(ctx)?.create_commands(vec![bookmark]).await {
		console_error!("Failed to create command: {e}");
		Response::error("Failed to create command", 500)
	} else {
		Response::empty()
	}
}
