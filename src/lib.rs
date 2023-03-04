use client::Client;
use ed25519_dalek::{ed25519::signature::Signature, PublicKey, Verifier};
use twilight_model::{
	application::{
		command::CommandType,
		interaction::{
			application_command::CommandData, Interaction, InteractionData,
			InteractionType,
		},
	},
	channel::message::MessageFlags,
	http::interaction::{InteractionResponse, InteractionResponseType},
	id::{marker::GuildMarker, Id},
	user::User,
};
use twilight_util::builder::{
	command::CommandBuilder, InteractionResponseDataBuilder,
};
use utils::Context as _;
use worker::*;

mod client;
mod utils;

const COMMAND_NAME: &str = "Bookmark message";

fn log_request(req: &Request) {
	console_log!(
		"{} - [{}], located at: {:?}, within: {}",
		Date::now().to_string(),
		req.path(),
		req.cf().coordinates().unwrap_or_default(),
		req.cf().region().unwrap_or("unknown region".into())
	);
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
	log_request(&req);

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
					Response::ok(serde_json::to_string(&resp)?)
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

					let user = interaction.user.context("No user provided")?;

					handle_command(*data, user).await
				}
				_ => Response::error("Unexpected interaction type", 400),
			}
		})
		.post_async("/register/*guild", |_, ctx| async move {
			register_command(ctx).await
		})
		.run(req, env)
		.await
}

async fn handle_command(command: CommandData, _user: User) -> Result<Response> {
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

	let _message = resolved
		.messages
		.get(&target.cast())
		.context("No resolved message")?;

	let response = InteractionResponse {
		kind: InteractionResponseType::ChannelMessageWithSource,
		data: Some(
			InteractionResponseDataBuilder::new()
				.flags(MessageFlags::EPHEMERAL)
				.content("Message bookmarked (in theory)")
				.build(),
		),
	};

	Response::ok(serde_json::to_string(&response)?)
}

fn check_signature<D>(
	req: &Request,
	body: &str,
	ctx: &RouteContext<D>,
) -> Result<Result<()>> {
	let public_key = ctx.var("DISCORD_PUBLIC_KEY")?.to_string();
	let public_key = hex::decode(&public_key).context("Non-hex public key")?;
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
	let signature = hex::decode(&signature).context("Non-hex signature")?;
	let signature =
		Signature::from_bytes(&signature).context("Invalid signature")?;

	public_key
		.verify(&signed, &signature)
		.context("Signature verification failed")?;

	Ok(())
}

async fn register_command<D>(ctx: RouteContext<D>) -> Result<Response> {
	let guild_id = ctx
		.param("guild")
		.and_then(|s| s.strip_prefix("/"))
		.filter(|s| !s.is_empty())
		.map(|s| s.parse::<Id<GuildMarker>>())
		.transpose()
		.context("Invalid ID")?;

	let id = ctx
		.var("DISCORD_APPLICATION_ID")?
		.to_string()
		.parse()
		.map_err(|e| {
			Error::RustError(format!("Failed to parse application id: {e}"))
		})?;

	let mut client = Client::new(ctx.var("DISCORD_TOKEN")?.to_string(), id);
	let command = CommandBuilder::new(COMMAND_NAME, "", CommandType::Message)
		.dm_permission(false)
		.build();

	if let Err(e) = client.create_command(command, guild_id).await {
		console_error!("Failed to create command: {e}");
		Response::error("Failed to create command", 500)
	} else {
		Response::empty()
	}
}
