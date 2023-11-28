#![warn(clippy::str_to_string)]

mod commands;
mod create;
mod role;
mod config;

use crate::config::CONFIG;
use crate::role::random_color;

use std::sync::Arc;
use std::time::Duration;
use serenity::all::{ActivityData, ClientBuilder, GatewayIntents, Permissions};
use serenity::builder::{CreateMessage, EditRole};

use poise::serenity_prelude as serenity;

type Error = Box<dyn std::error::Error + Send + Sync>;
type PoiseContext<'a> = poise::Context<'a, Data, Error>;

pub struct Data {}

static mut FIRST_TIME: bool = true;

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        poise::FrameworkError::Command { error, ctx, .. } => {
            println!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                println!("Error while handling error: {}", e)
            }
        }
    }
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    _: &Data,
) -> Result<(), Error> {
    match event {
        serenity::FullEvent::Ready { data_about_bot, .. } => unsafe {
            println!("Logged in as {}", data_about_bot.user.name);
            ctx.set_activity(Some(ActivityData::playing("Perfect Color!")));

            if FIRST_TIME {
                for guild in ctx.cache.guilds() {
                    let existing_roles: Vec<String> = guild.roles(&ctx.http).await?.values().map(|role| role.name.clone()).collect();
                    for (name, color) in &CONFIG.colors {
                        if existing_roles.contains(name) {
                            continue;
                        }

                        let r = EditRole::new().name(name).colour(color.clone()).permissions(Permissions::empty());
                        match guild.create_role(&ctx.http, r).await {
                            Ok(_) => {}
                            Err(e) => {
                                println!("Error while creating colors on this server {} - {}, {}", guild.get(), guild.name(&ctx.cache).unwrap(), e);
                                break;
                            }
                        };
                    }
                }
                FIRST_TIME = false;
            }
            println!("Bot is ready")
        }
        serenity::FullEvent::GuildMemberAddition { new_member } => {
            match random_color(&ctx, &new_member).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error while member join, {}", e);
                    return Ok(())
                }
            }
            if !CONFIG.auto_kick.contains_key(new_member.guild_id.to_string().as_str()) {
                println!("Server not in kick list");
                return Ok(())
            }

            tokio::time::sleep(Duration::from_secs(30 * 60)).await;

            match new_member.roles(&ctx.cache) {
                Some(roles) => {
                    for role in roles {
                        if CONFIG.colors.contains_key(role.name.as_str()) {
                            continue;
                        } else {
                            // User has at least one role that isn't a color
                            return Ok(())
                        }
                    }
                },
                None => {
                    eprintln!("Unable to retrieve roles from cache");
                    return Ok(())
                }
            }

            match new_member.user.dm(&ctx.http, CreateMessage::new().content(format!("You got kicked from the server.\nPlease read the welcome channel for more information\n{}", CONFIG.invite_link))).await {
                Ok(_) => {},
                Err(e) => {
                    println!("Unable to message user in private chat, {}", e);
                    return Ok(())
                }
            };

            match new_member.kick_with_reason(&ctx.http, "User hasn't picked a role after 30 minutes").await {
                Ok(_) => {},
                Err(e) => {
                    println!("Unable to kick user after 30 minutes, {}", e);
                    return Ok(())
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    env_logger::init();

    if CONFIG.bot_token == "YOUR_BOT_TOKEN" {
        eprintln!("Your bot token is still the default, please change it in the config.yaml");
        return;
    }

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_MEMBERS;
    let options = poise::FrameworkOptions {
        commands: vec![commands::color(), commands::preview(), commands::help()],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("<<".into()),
            edit_tracker: Some(Arc::from(poise::EditTracker::for_timespan(Duration::from_secs(3600)))),
            ..Default::default()
        },
        on_error: |error| Box::pin(on_error(error)),
        skip_checks_for_owners: false,
        event_handler: |ctx, event, framework, data| {
            Box::pin(event_handler(ctx, event, framework, data))
        },
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        })
        .options(options)
        .build();

    let client = ClientBuilder::new(CONFIG.bot_token.as_str(), intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}