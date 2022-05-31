use std::fs;
use std::io::Write;

use serde_derive::Deserialize;
use serde_json::Value;

use reqwest::get;

use serenity::async_trait;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{CommandResult, StandardFramework};
use serenity::model::channel::Message;
use serenity::model::gateway::Activity;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::utils::Color;

#[derive(Deserialize)]
struct Config {
    discord_token: String,
    command_prefix: String,
    openai_key: String,
    admin_role: u64,
}

// To insert Admin Role in Client using TypeMap
struct Admin;
impl TypeMapKey for Admin {
    type Value = u64;
}
// To insert OpenAI API Key in Client using TypeMap
struct OpenAI;
impl TypeMapKey for OpenAI {
    type Value = String;
}

#[group]
#[commands(ping, meme, gif, details, chat, help)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content.to_lowercase().starts_with("hello ru") {
            if let Err(e) = msg.reply(ctx, format!("Hello {}", msg.author.name)).await {
                eprintln!("[ Error ] {}", e);
            }
        }
    }

    async fn ready(&self, ctx: Context, msg: Ready) {
        println!("[ Ready ] {} is connected.", msg.user.name);
        if let Ok(guilds) = msg.user.guilds(&ctx.http).await {
            for guild in guilds.into_iter() {
                println!("    - {}", guild.name);
            }
        };
        ctx.set_presence(
            Some(Activity::listening("Ru help")),
            serenity::model::user::OnlineStatus::Idle,
        )
        .await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string("config.toml")?;
    let config: Config = toml::from_str(&content)?;

    let framework = StandardFramework::new()
        .configure(|c| c.prefix(&config.command_prefix).with_whitespace(true))
        .group(&GENERAL_GROUP);

    let intents = GatewayIntents::all();
    let mut client = Client::builder(config.discord_token, intents)
        .event_handler(Handler)
        .type_map(TypeMap::new())
        .type_map_insert::<Admin>(config.admin_role)
        .type_map_insert::<OpenAI>(config.openai_key)
        .framework(framework)
        .await?;

    tokio::spawn(async move {
        let _ = client
            .start()
            .await
            .map_err(|e| eprintln!("[ Error ] {}", e));
    });

    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("[ Error ] {}", e);
    }
    println!("\n[ Signal ] Received Ctrl + C, Shutting Down.");

    Ok(())
}

#[command]
async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    let url = ctx.cache.current_user().avatar_url();

    msg.channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.title("Rusty Help Information:")
                    .field("Ru help", "Show this message", true)
                    .field("Ru gif", "Respond with meme gif", true)
                    .field("Ru meme", "Respond with meme image", true)
                    .field("Ru ping", "Respond with Pong!", true)
                    .field(
                        "Ru chat",
                        "Respond with response obtained through OpenAI",
                        true,
                    )
                    .field(
                        "Ru details",
                        "Respond with Server Details [Admin role Only]",
                        true,
                    );
                if let Some(url) = url {
                    e.thumbnail(url)
                } else {
                    e
                }
            })
        })
        .await?;
    Ok(())
}

#[command]
async fn details(ctx: &Context, msg: &Message) -> CommandResult {
    let roles = &msg.member.as_ref().unwrap().roles;
    for role in roles.iter() {
        if &role.0 == ctx.data.read().await.get::<Admin>().unwrap() {
            let guild = ctx.http.get_guild(msg.guild_id.unwrap().0).await?;
            let server_name = &guild.name;
            let thumbnail = &guild.icon_url().unwrap_or("No Icon".to_owned());
            let owner = ctx
                .http
                .get_member(msg.guild_id.unwrap().0, guild.owner_id.0)
                .await?;
            let members = guild.members(ctx, Some(500), None).await?;
            let members_count = members.iter().len();

            msg.channel_id
                .send_message(ctx, |f| {
                    f.embed(|e| {
                        e.title(format!("{} Server's Info:", server_name))
                            .field("Owner", owner, true)
                            .field("Server ID", guild.id.0, true)
                            .field("MemberCount", members_count, true)
                            .color(Color::FADED_PURPLE)
                            .thumbnail(thumbnail)
                    })
                })
                .await?;

            for member in members.into_iter() {
                let content = format!(
                    "Member name: {}\nID :{}\nJoined at: {} ",
                    &member.user.name,
                    &member.user.id,
                    &member.joined_at.unwrap()
                )
                .to_string();
                msg.channel_id
                    .send_message(ctx, |f| f.content(content))
                    .await?;
            }
        } else {
            msg.reply(ctx, "You don't have Required Role.").await?;
        }
    }
    Ok(())
}

#[command]
async fn chat(ctx: &Context, msg: &Message) -> CommandResult {
    let openai_token = ctx.data.read().await.get::<OpenAI>().unwrap().clone();
    let client = openai_api_fork::Client::new(&openai_token);

    let mut prompt = fs::read_to_string("prompt.txt")?;
    prompt = format!("{}You: {}\n", prompt, msg.content);

    let args = openai_api_fork::api::CompletionArgs::builder()
        .prompt(&prompt)
        .engine("davinci")
        .max_tokens(256)
        .temperature(0.5)
        .top_p(0.3)
        .n(1)
        .presence_penalty(0.0)
        .frequency_penalty(0.5)
        .stop(vec!["\nYou:".into(), "\nRu:".into()])
        .build()?;
    let response = client.complete_prompt(args).await?;
    if let Err(e) = msg
        .reply(ctx, &response.choices[0].text.replace("Ru:", ""))
        .await
    {
        eprintln!("[ Error ] {}", e);
    }

    let mut file = fs::File::options().append(true).open("prompt.txt")?;
    if let Err(e) = write!(file, "You: {}\n{}\n", msg.content, response.choices[0].text) {
        eprintln!("[ Error ] {}", e);
    }

    Ok(())
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    if let Err(why) = msg.channel_id.broadcast_typing(&ctx).await {
        eprintln!("[ Error ] {}", why);
    }

    msg.reply(ctx, "Pong!").await?;
    Ok(())
}

#[command]
async fn meme(ctx: &Context, msg: &Message) -> CommandResult {
    if let Err(why) = msg.channel_id.broadcast_typing(&ctx).await {
        eprintln!("[ Error ] {}", why);
    }

    let resp = get("https://meme-api.herokuapp.com/gimme")
        .await?
        .text()
        .await?;

    let resp: Value = serde_json::from_str(resp.as_str())?;
    let url = format!("{}", resp["preview"][3]).replace("\"", "");

    msg.reply(ctx, url).await?;

    Ok(())
}

#[command]
async fn gif(ctx: &Context, msg: &Message) -> CommandResult {
    if let Err(why) = msg.channel_id.broadcast_typing(&ctx).await {
        eprintln!("[ Error ] {}", why);
    }

    let resp = get("https://meme-api.herokuapp.com/gimme")
        .await?
        .text()
        .await?;
    let resp: Value = serde_json::from_str(resp.as_str())?;
    let title = format!("{}", resp["title"]).replace("\"", "");
    let url = format!("{}", resp["url"]).replace("\"", "");

    msg.channel_id
        .send_message(&ctx.http, |m| {
            m.embed(|e| e.title(title).image(url).color(Color::DARK_GREY))
        })
        .await?;

    Ok(())
}
