use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::Path,
    time::Duration,
};

use actionable::{Action, ActionNameList, Permissions, ResourceName, Statement};
use bonsaidb::{
    core::{
        async_trait::async_trait,
        connection::StorageConnection,
        custom_api::Infallible,
        permissions::bonsai::{BonsaiAction, ServerAction},
        schema::{Collection, CollectionDocument},
    },
    server::{
        cli::Command, Backend, BackendError, Configuration, ConnectedClient, ConnectionHandling,
        CustomApiDispatcher, CustomServer, DefaultPermissions, ServerDatabase,
    },
};
use minority_game_shared::{
    Api, Choice, Request, RequestDispatcher, Response, SetChoiceHandler, SetTellHandler,
};
use rand::{thread_rng, Rng};
use structopt::StructOpt;
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
    let command = Command::<Game>::from_args();

    let server = CustomServer::<Game>::open(
        Path::new("minority-game.bonsaidb"),
        Configuration {
            server_name: String::from("minority-game.gooey.rs"),
            default_permissions: DefaultPermissions::Permissions(Permissions::from(vec![
                Statement {
                    resources: vec![ResourceName::any()],
                    actions: ActionNameList::List(vec![BonsaiAction::Server(
                        ServerAction::Connect,
                    )
                    .name()]),
                },
            ])),
            ..Configuration::default()
        },
    )
    .await?;

    match command {
        Command::Certificate(cert_command) => {
            let is_installing_self_signed = matches!(
                cert_command,
                bonsaidb::server::cli::certificate::Command::InstallSelfSigned { .. }
            );
            cert_command.execute(server.clone()).await?;
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
                .execute_with(server.clone(), WebServer::new(server).await)
                .await?
        }
    }
    Ok(())
}

#[derive(Debug)]
enum Game {}

#[async_trait]
impl Backend for Game {
    type ClientData = CollectionDocument<Player>;
    type CustomApi = Api;
    type CustomApiDispatcher = ApiDispatcher;

    async fn initialize(server: &CustomServer<Self>) {
        server.register_schema::<GameSchema>().await.unwrap();
        server
            .create_database::<GameSchema>(DATABASE_NAME, true)
            .await
            .unwrap();

        tokio::spawn(game_loop(server.clone()));
    }

    async fn client_connected(
        client: &ConnectedClient<Self>,
        server: &CustomServer<Self>,
    ) -> ConnectionHandling {
        log::info!(
            "{:?} client connected from {:?}",
            client.transport(),
            client.address()
        );

        let player = Player::default()
            .insert_into(&server.game_database().await.unwrap())
            .await
            .unwrap();
        client.set_client_data(player).await;

        ConnectionHandling::Accept
    }
}

impl CustomApiDispatcher<Game> for ApiDispatcher {
    fn new(server: &CustomServer<Game>, client: &ConnectedClient<Game>) -> Self {
        ApiDispatcher {
            server: server.clone(),
            client: client.clone(),
        }
    }
}

#[derive(Debug, actionable::Dispatcher)]
#[dispatcher(input = Request)]
struct ApiDispatcher {
    server: CustomServer<Game>,
    client: ConnectedClient<Game>,
}

impl RequestDispatcher for ApiDispatcher {
    type Error = BackendError<Infallible>;
    type Output = Response;
}

#[actionable::async_trait]
impl SetChoiceHandler for ApiDispatcher {
    async fn handle(
        &self,
        _permissions: &actionable::Permissions,
        choice: Choice,
    ) -> Result<Response, BackendError<Infallible>> {
        let db = self.server.game_database().await?;

        let mut player = self.client.client_data().await;
        let player = player
            .as_mut()
            .expect("all connected clients should have a player record");

        player.contents.choice = Some(choice);
        player.update(&db).await?;

        Ok(Response::ChoiceSet(choice))
    }
}

#[actionable::async_trait]
impl SetTellHandler for ApiDispatcher {
    async fn handle(
        &self,
        _permissions: &actionable::Permissions,
        tell: Choice,
    ) -> Result<Response, BackendError<Infallible>> {
        let db = self.server.game_database().await?;

        let mut player = self.client.client_data().await;
        let player = player
            .as_mut()
            .expect("all connected clients should have a player record");

        player.contents.tell = Some(tell);
        player.update(&db).await?;

        Ok(Response::ChoiceSet(tell))
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
        let client = &clients_by_player_id[&player.id];
        drop(client.send(Ok(Response::RoundPending {
            seconds_remaining,
            number_of_players: players.len() as u32,
            current_rank: index as u32 + 1,
            tells_going_out,
            number_of_tells,
        })));
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
                going_out_player_ids.insert(player.id);
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
        player.update(db).await?;
    }

    sort_players(&mut players);

    let number_of_players = players.len() as u32;
    for (index, player) in players.into_iter().enumerate() {
        let client = &clients_by_player_id[&player.id];
        let won = if going_out_player_ids.contains(&player.id) {
            had_fun
        } else {
            player.contents.stats.happiness < 0.5
        };
        drop(client.send(Ok(Response::RoundComplete {
            won,
            happiness: player.contents.stats.happiness,
            current_rank: index as u32 + 1,
            number_of_players,
            number_of_tells,
            number_of_liars,
        })));
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
            clients_by_player_id.insert(player.id, client.clone());
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
