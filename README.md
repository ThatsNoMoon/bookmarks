# bookmarks

bookmarks is a simple bookmarking bot for Discord, designed for Cloudflare Workers and written in Rust. It uses message commands, available via the right click menu on any message. You'll be sent a direct message with the contents of the message.

## Public Bot
You can add bookmarks to your server using this [invite link](https://discord.com/api/oauth2/authorize?client_id=1080248268023398430&permissions=0&scope=bot%20applications.commands).

## Setup
Install and configure [`wrangler`](https://developers.cloudflare.com/workers/wrangler/).

Copy `wrangler.example.toml` to `wrangler.toml` and fill in the application ID and public key from the Discord developer portal.

Use `wrangler secret put` to save `DISCORD_TOKEN` (from the Discord developer portal) and `REGISTRATION_TOKEN` (see below).

### Command Registration
Because Cloudflare Workers are serverless, there is no startup in which to register commands. You must register the bookmark command manually one time.

Given your worker is operating on `example.yourusername.workers.dev`, use `curl` or another HTTP client to make a POST request to `https://example.yourusername.workers.dev/register`. If you set the (optional, but recommended) `REGISTRATION_TOKEN` secret, an `Authorization` header containing the token will be required, preventing others from accessing the Discord API through your account. The token can contain any text desired.

For example:
```
curl -H "Authorization: gjsoW9XUoRTRvSYv" -X POST https://example.yourusername.workers.dev
```
