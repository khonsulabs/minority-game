use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    time::Duration,
};

use actionable::{Permissions, Statement};
use bonsaidb::{
    core::{
        api::Infallible,
        async_trait::async_trait,
        connection::AsyncStorageConnection,
        document::CollectionDocument,
        permissions::bonsai::{BonsaiAction, ServerAction},
        schema::SerializedCollection,
    },
    local::{config::Builder, StorageNonBlocking},
    server::{
        api::{Handler, HandlerError, HandlerSession},
        cli::Command,
        Backend, BackendError, ConnectedClient, ConnectionHandling, CustomServer,
        ServerConfiguration, ServerDatabase,
    },
};
use clap::Parser;
use minority_game_shared::{
    Choice, ChoiceSet, RoundComplete, RoundPending, SetChoice, SetTell, Welcome,
};
use rand::{thread_rng, Rng};
use tokio::time::Instant;

use crate::{
    schema::{GameSchema, Player},
    webserver::WebServer,
};

mod schema;
mod webserver;

const DATABASE_NAME: &str = "minority-game";
const SECONDS_PER_ROUND: u32 = 5;
const TOO_BUSY_HAPPINESS_MULTIPLIER: f32 = 0.8;
const HAD_FUN_HAPPINESS_MULTIPLIER: f32 = 1.5;
const STAYED_IN_MULTIPLIER: f32 = 0.2;

#[tokio::main]
#[cfg_attr(not(debug_assertions), allow(unused_mut))]
async fn main() -> anyhow::Result<()> {
    let command = Command::<Game>::parse();

    let server = CustomServer::<Game>::open(
        ServerConfiguration::new("minority-game.bonsaidb")
            .server_name("minority-game.gooey.rs")
            .default_permissions(Permissions::from(
                Statement::for_any().allowing(&BonsaiAction::Server(ServerAction::Connect)),
            )),
    )
    .await?;

    match command {
        Command::Certificate(cert_command) => {
            let is_installing_self_signed = matches!(
                cert_command,
                bonsaidb::server::cli::certificate::Command::InstallSelfSigned { .. }
            );
            cert_command.execute(&server).await?;
            if is_installing_self_signed {
                if let Ok(chain) = server.certificate_chain().await {
                    tokio::fs::write(
                        server.path().join("public-certificate.der"),
                        &chain.end_entity_certificate(),
                    )
                    .await?;
                }
            }
        }
        Command::Serve(mut serve_command) => {
            #[cfg(debug_assertions)]
            if serve_command.http_port.is_none() {
                use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

                serve_command.http_port = Some(SocketAddr::V6(SocketAddrV6::new(
                    Ipv6Addr::UNSPECIFIED,
                    8080,
                    0,
                    0,
                )));
                serve_command.https_port = Some(SocketAddr::V6(SocketAddrV6::new(
                    Ipv6Addr::UNSPECIFIED,
                    8081,
                    0,
                    0,
                )));
            }

            serve_command
                .execute_with(&server, WebServer::new(server.clone()).await)
                .await?
        }
        Command::Storage(storage) => storage.execute_on_async(&server).await?,
    }
    Ok(())
}

#[derive(Debug)]
enum Game {}

#[async_trait]
impl Backend for Game {
    type Error = Infallible;
    type ClientData = CollectionDocument<Player>;

    fn configure(
        config: ServerConfiguration<Self>,
    ) -> Result<ServerConfiguration<Self>, BackendError<Infallible>> {
        Ok(config
            .with_schema::<GameSchema>()?
            .with_api::<ApiHandler, SetChoice>()?
            .with_api::<ApiHandler, SetTell>()?)
    }

    async fn initialize(server: &CustomServer<Self>) -> Result<(), BackendError<Infallible>> {
        server
            .create_database::<GameSchema>(DATABASE_NAME, true)
            .await?;

        tokio::spawn(game_loop(server.clone()));
        Ok(())
    }

    async fn client_connected(
        client: &ConnectedClient<Self>,
        server: &CustomServer<Self>,
    ) -> Result<ConnectionHandling, BackendError<Infallible>> {
        log::info!(
            "{:?} client connected from {:?}",
            client.transport(),
            client.address()
        );

        let player = Player::default()
            .push_into_async(&server.game_database().await?)
            .await?;

        drop(client.send::<Welcome>(
            None,
            &Welcome {
                player_id: player.header.id,
                happiness: player.contents.stats.happiness,
            },
        ));

        client.set_client_data(player).await;

        Ok(ConnectionHandling::Accept)
    }
}

#[derive(Debug)]
enum ApiHandler {}

#[actionable::async_trait]
impl Handler<Game, SetChoice> for ApiHandler {
    async fn handle(
        session: HandlerSession<'_, Game>,
        api: SetChoice,
    ) -> Result<ChoiceSet, HandlerError<Infallible>> {
        let SetChoice(choice) = api;
        let db = session.server.game_database().await?;

        let mut player = session.client.client_data().await;
        let player = player
            .as_mut()
            .expect("all connected clients should have a player record");

        player.contents.choice = Some(choice);
        player.update_async(&db).await?;

        Ok(ChoiceSet(choice))
    }
}

#[actionable::async_trait]
impl Handler<Game, SetTell> for ApiHandler {
    async fn handle(
        session: HandlerSession<'_, Game>,
        api: SetTell,
    ) -> Result<ChoiceSet, HandlerError<Infallible>> {
        let SetTell(tell) = api;
        let db = session.server.game_database().await?;

        let mut player = session.client.client_data().await;
        let player = player
            .as_mut()
            .expect("all connected clients should have a player record");

        player.contents.tell = Some(tell);
        player.update_async(&db).await?;

        Ok(ChoiceSet(tell))
    }
}

#[async_trait]
trait CustomServerExt {
    async fn game_database(&self) -> Result<ServerDatabase<Game>, bonsaidb::core::Error>;
}

#[async_trait]
impl CustomServerExt for CustomServer<Game> {
    async fn game_database(&self) -> Result<ServerDatabase<Game>, bonsaidb::core::Error> {
        self.database::<GameSchema>(DATABASE_NAME).await
    }
}

async fn game_loop(server: CustomServer<Game>) -> Result<(), bonsaidb::server::Error> {
    let mut last_iteration = Instant::now();
    let mut state = GameState::Idle;
    let db = server.game_database().await?;
    loop {
        last_iteration += Duration::from_secs(1);
        tokio::time::sleep_until(last_iteration).await;

        let clients = server.connected_clients().await;

        state = match state {
            GameState::Idle => send_status_update(&clients, None).await?,
            GameState::Pending {
                mut seconds_remaining,
            } => {
                if seconds_remaining > 0 {
                    seconds_remaining -= 1;
                    send_status_update(&clients, Some(seconds_remaining)).await?
                } else {
                    play_game(&db, &clients).await?
                }
            }
        };
    }
}

async fn send_status_update(
    clients: &[ConnectedClient<Game>],
    seconds_remaining: Option<u32>,
) -> Result<GameState, bonsaidb::server::Error> {
    let (mut players, clients_by_player_id) = collect_players(clients).await?;
    if players.is_empty() {
        return Ok(GameState::Idle);
    }

    sort_players(&mut players[..]);

    let (tells_going_out, number_of_tells) = players
        .iter()
        .map(|player| match player.contents.tell {
            Some(Choice::GoOut) => (1, 1),
            Some(Choice::StayIn) => (0, 1),
            None => (0, 0),
        })
        .fold((0, 0), |acc, player| (acc.0 + player.0, acc.1 + player.1));

    let seconds_remaining = seconds_remaining.unwrap_or(SECONDS_PER_ROUND);

    for (index, player) in players.iter().enumerate() {
        let client = &clients_by_player_id[&player.header.id];
        drop(client.send::<RoundPending>(
            None,
            &RoundPending {
                seconds_remaining,
                number_of_players: players.len() as u32,
                current_rank: index as u32 + 1,
                tells_going_out,
                number_of_tells,
            },
        ));
    }

    Ok(GameState::Pending { seconds_remaining })
}

async fn play_game(
    db: &ServerDatabase<Game>,
    clients: &[ConnectedClient<Game>],
) -> Result<GameState, bonsaidb::server::Error> {
    let (mut players, clients_by_player_id) = collect_players(clients).await?;
    if players.is_empty() {
        return Ok(GameState::Idle);
    }

    let mut going_out_player_ids = HashSet::new();
    let mut going_out = 0_u32;
    let mut staying_in = 0_u32;
    for player in &players {
        match player.contents.choice.unwrap() {
            Choice::GoOut => {
                going_out_player_ids.insert(player.header.id);
                going_out += 1;
            }
            Choice::StayIn => {
                staying_in += 1;
            }
        }
    }

    {
        let mut rng = thread_rng();
        while going_out + staying_in < 3 {
            if rng.gen_bool(0.5) {
                going_out += 1;
            } else {
                staying_in += 1;
            }
        }
    }

    let (number_of_liars, number_of_tells) = players
        .iter()
        .map(|player| {
            if player.contents.tell.is_some() && player.contents.choice != player.contents.tell {
                (1, 1)
            } else {
                (0, if player.contents.tell.is_some() { 1 } else { 0 })
            }
        })
        .fold((0, 0), |acc, player| (acc.0 + player.0, acc.1 + player.1));

    let had_fun = going_out <= staying_in;
    for player in &mut players {
        match player.contents.choice.take().unwrap() {
            Choice::GoOut => {
                player.contents.stats.times_went_out += 1;
                if had_fun {
                    player.contents.stats.happiness =
                        (player.contents.stats.happiness * HAD_FUN_HAPPINESS_MULTIPLIER).min(1.);
                } else {
                    player.contents.stats.happiness *= TOO_BUSY_HAPPINESS_MULTIPLIER;
                }
            }
            Choice::StayIn => {
                player.contents.stats.times_stayed_in += 1;
                player.contents.stats.happiness = (player.contents.stats.happiness
                    + (0.5 - player.contents.stats.happiness) * STAYED_IN_MULTIPLIER)
                    .min(1.);
            }
        }

        player.contents.tell = None;
        player.update_async(db).await?;
    }

    sort_players(&mut players);

    let number_of_players = players.len() as u32;
    for (index, player) in players.into_iter().enumerate() {
        let client = &clients_by_player_id[&player.header.id];
        let won = if going_out_player_ids.contains(&player.header.id) {
            had_fun
        } else {
            player.contents.stats.happiness < 0.5
        };
        drop(client.send::<RoundComplete>(
            None,
            &RoundComplete {
                won,
                happiness: player.contents.stats.happiness,
                current_rank: index as u32 + 1,
                number_of_players,
                number_of_tells,
                number_of_liars,
            },
        ));
        client.set_client_data(player).await;
    }

    Ok(GameState::Idle)
}

enum GameState {
    Idle,
    Pending { seconds_remaining: u32 },
}

async fn collect_players(
    clients: &[ConnectedClient<Game>],
) -> Result<
    (
        Vec<CollectionDocument<Player>>,
        HashMap<u64, ConnectedClient<Game>>,
    ),
    bonsaidb::server::Error,
> {
    let mut players = Vec::new();
    let mut clients_by_player_id = HashMap::new();

    for client in clients {
        let mut player = client.client_data().await;
        if let Some(player) = player.as_mut() {
            clients_by_player_id.insert(player.header.id, client.clone());
            if player.contents.choice.is_some() {
                players.push(player.clone());
            }
        }
    }

    Ok((players, clients_by_player_id))
}

fn sort_players(players: &mut [CollectionDocument<Player>]) {
    players.sort_by(|a, b| {
        assert!(!a.contents.stats.happiness.is_nan() && !b.contents.stats.happiness.is_nan());
        if approx::relative_eq!(a.contents.stats.happiness, b.contents.stats.happiness) {
            Ordering::Equal
        } else if a.contents.stats.happiness < b.contents.stats.happiness {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    });
}
