use std::time::Duration;

use bonsaidb::client::Client;
use cfg_if::cfg_if;
use gooey::{
    core::{figures::Size, Context, StyledWidget, WindowBuilder},
    widgets::{
        button::Button,
        component::{Behavior, Component, ComponentCommand, Content, EventMapper},
        label::Label,
        layout::{Dimension, Layout, WidgetLayout},
    },
    App,
};
use minority_game_shared::{whole_percent, Api, Choice, Request, Response};

fn main() {
    // The user interface and database will be run separately, and flume
    // channels will send `DatabaseCommand`s to do operations on the database
    // server.
    let (command_sender, command_receiver) = flume::unbounded();

    // Spawn an async task that processes commands sent by `command_sender`.
    App::spawn(process_database_commands(command_receiver));

    App::from(
        WindowBuilder::new(|storage| Component::new(GameInterface::new(command_sender), storage))
            .size(Size::new(512, 384))
            .title("Minority Game - BonsaiDb + Gooey Demo"),
    )
    // Register our custom component's transmogrifier.
    .with_component::<GameInterface>()
    // Run the app using the widget returned by the initializer.
    .run()
}

#[derive(Debug)]
struct GameInterface {
    command_sender: flume::Sender<DatabaseCommand>,
}

impl GameInterface {
    /// Returns a new instance that sends database commands to `command_sender`.
    pub const fn new(command_sender: flume::Sender<DatabaseCommand>) -> Self {
        Self { command_sender }
    }
}

/// Component defines a trait `Behavior` that allows you to write cross-platform
/// code that interacts with one or more other widgets.
impl Behavior for GameInterface {
    type Content = Layout;
    /// The event enum that child widget events will send.
    type Event = GameInterfaceEvent;
    /// An enum of child widgets.
    type Widgets = GameWidgets;

    fn build_content(
        &mut self,
        builder: <Self::Content as Content<Self>>::Builder,
        events: &EventMapper<Self>,
    ) -> StyledWidget<Layout> {
        builder
            .with(
                None,
                Label::new("This is an adaption of the game theory game \"Minority Game\". Choose between staying in or going out. If more than 50% of the players go out, everyone who goes out will lose happiness because they have a bad time when everything is crowded. However, if it's not too crowed, the players who chose to go out will gain a significant amount of happiness. Those who choose to stay in will gravitate toward 50% happiness."),
                WidgetLayout::build()
                    .top(Dimension::exact(20.))
                    .left(Dimension::exact(20.))
                    .right(Dimension::exact(20.))
                    .finish(),
            )
            .with(
                None,
                Label::new("Pick your choice:"),
                WidgetLayout::build()
                .bottom(Dimension::exact(100.))
                    .left(Dimension::exact(20.))
                    .finish(),
            )
            .with(
                GameWidgets::GoOut,
                Button::new(
                    "Go Out",
                    events.map(|_| GameInterfaceEvent::ChoiceClicked(Choice::GoOut)),
                ),
                WidgetLayout::build()
                    .left(Dimension::exact(200.))
                    .bottom(Dimension::exact(100.))
                    .finish(),
            )
            .with(
                GameWidgets::StayIn,
                Button::new(
                    "Stay In",
                    events.map(|_| GameInterfaceEvent::ChoiceClicked(Choice::StayIn)),
                ),
                WidgetLayout::build()
                    .left(Dimension::exact(250.))
                    .bottom(Dimension::exact(100.))
                    .finish(),
            )
            .with(
                None,
                Label::new("Pick your tell (Optional):"),
                WidgetLayout::build()
                    .bottom(Dimension::exact(60.))
                    .left(Dimension::exact(20.))
                    .finish(),
            )
            .with(
                GameWidgets::TellGoOut,
                Button::new(
                    "Go Out",
                    events.map(|_| GameInterfaceEvent::TellClicked(Choice::GoOut)),
                ),
                WidgetLayout::build()
                    .left(Dimension::exact(200.))
                    .bottom(Dimension::exact(60.))
                        .finish(),
            )
            .with(
                GameWidgets::TellStayIn,
                Button::new(
                    "Stay In",
                    events.map(|_| GameInterfaceEvent::TellClicked(Choice::StayIn)),
                ),
                WidgetLayout::build()
                .left(Dimension::exact(250.))
                .bottom(Dimension::exact(60.))
                    .finish(),
            )
            .with(
                GameWidgets::Status,
                Label::new("Connecting..."),
                WidgetLayout::build()
                    .bottom(Dimension::exact(20.))
                    .left(Dimension::exact(20.))
                    .right(Dimension::exact(20.))
                    .finish(),
            )
            .finish()
    }

    fn initialize(component: &mut Component<Self>, context: &Context<Component<Self>>) {
        let _ = component
            .behavior
            .command_sender
            .send(DatabaseCommand::Initialize(DatabaseContext {
                context: context.clone(),
            }));
    }

    fn receive_event(
        component: &mut Component<Self>,
        event: Self::Event,
        context: &Context<Component<Self>>,
    ) {
        match event {
            GameInterfaceEvent::ChoiceClicked(choice) => {
                let _ = component
                    .behavior
                    .command_sender
                    .send(DatabaseCommand::SetChoice(choice));
            }
            GameInterfaceEvent::TellClicked(choice) => {
                let _ = component
                    .behavior
                    .command_sender
                    .send(DatabaseCommand::SetTell(choice));
            }
            GameInterfaceEvent::UpdateStatus(status) => {
                let label = component
                    .widget_state(&GameWidgets::Status, context)
                    .unwrap();
                let mut label = label.lock::<Label>(context.frontend()).unwrap();
                label.widget.set_label(status, &label.context);
            }
        }
    }
}

/// This enum identifies widgets that you want to send commands to. If a widget
/// doesn't need to receive commands, it doesn't need an entry in this enum.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
enum GameWidgets {
    GoOut,
    StayIn,
    Status,
    TellGoOut,
    TellStayIn,
}

#[derive(Debug)]
enum GameInterfaceEvent {
    ChoiceClicked(Choice),
    TellClicked(Choice),
    UpdateStatus(String),
}

/// Commands that the user interface will send to the database task.
enum DatabaseCommand {
    /// Initializes the worker with a context, which
    Initialize(DatabaseContext),
    SetChoice(Choice),
    SetTell(Choice),
}

/// A context provides the information necessary to communicate with the user
/// inteface.
#[derive(Clone)]
struct DatabaseContext {
    /// The context of the component.
    context: Context<Component<GameInterface>>,
}

async fn client() -> bonsaidb::client::Builder<()> {
    cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            cfg_if!{
                if #[cfg(debug_assertions)] {
                    Client::build("ws://127.0.0.1:8081".parse().unwrap())
                } else {
                    Client::build("wss://minority-game.gooey.rs/ws".parse().unwrap())
                }
            }
        } else {
            // Native
            cfg_if!{
                if #[cfg(debug_assertions)] {
                    let certificate = tokio::fs::read("minority-game.bonsaidb/public-certificate.der").await.unwrap();
                    Client::build("bonsaidb://127.0.0.1".parse().unwrap()).with_certificate(bonsaidb::client::fabruic::Certificate::from_der(certificate).unwrap())
                } else {
                    Client::build("bonsaidb://minority-game.gooey.rs".parse().unwrap())
                }
            }

        }
    }
}

/// Processes each command from `receiver` as it becomes available.
async fn process_database_commands(receiver: flume::Receiver<DatabaseCommand>) {
    let database = match receiver.recv_async().await.unwrap() {
        DatabaseCommand::Initialize(context) => context,
        _ => unreachable!(),
    };

    // Connect to the locally running server. `cargo run --package server`
    // launches the server.
    let client = loop {
        let api_callback_context = database.clone();
        match client().await
            .with_custom_api_callback::<Api, _>(move |response| match response {
                Ok(Response::Welcome {
                    happiness,
                    player_id
                }) => {
                    let _ = api_callback_context.context.send_command(ComponentCommand::Behavior(GameInterfaceEvent::UpdateStatus(
                        format!("Welcome {}! Current happiness: {}",
                            player_id,
                            whole_percent(happiness),
                        )
                    )));
                }
                Ok(Response::ChoiceSet(_)) => unreachable!(),
                Ok(Response::RoundComplete {
                    won,
                    happiness,
                    current_rank,
                    number_of_players,
                    number_of_liars,
                    number_of_tells
                }) => {
                    let _ = api_callback_context.context.send_command(ComponentCommand::Behavior(GameInterfaceEvent::UpdateStatus(
                        format!("You {}! {}/{} players lied about their intentions. Current happiness: {}%. Ranked {} of {} players in the last round.",
                            if won {
                                "won"
                            } else {
                                "lost"
                            },
                            number_of_liars,
                            number_of_tells,
                            whole_percent(happiness),
                            current_rank,
                            number_of_players
                        )
                    )));
                }
                Ok(Response::RoundPending {
                    current_rank,
                    number_of_players,
                    seconds_remaining,
                    number_of_tells,
                    tells_going_out,
                }) => {
                    let _ = api_callback_context.context.send_command(ComponentCommand::Behavior(GameInterfaceEvent::UpdateStatus(
                        format!("Round starting in {} seconds! Ranked {} of {}. Current tells: {}/{} ({}%) going out.",
                            seconds_remaining,
                            current_rank,
                            number_of_players,
                            tells_going_out,
                            number_of_tells,
                            whole_percent(tells_going_out as f32 / number_of_tells as f32)
                        )
                    )));
                }

                Err(err) => {
                    log::error!("Error from API: {:?}", err);
                }
            })
            .finish()
            .await
        {
            Ok(client) => break client,
            Err(err) => {
                log::error!("Error connecting: {:?}", err);
                App::sleep_for(Duration::from_secs(1)).await;
            }
        }
    };

    // For each `DatabaseCommand`. The only error possible from recv_async() is
    // a disconnected error, which should only happen when the app is shutting
    // down.
    while let Ok(command) = receiver.recv_async().await {
        match command {
            DatabaseCommand::SetChoice(choice) => {
                match client.send_api_request(Request::SetChoice(choice)).await {
                    Ok(Response::ChoiceSet(choice)) => {
                        log::info!("Choice confirmed: {:?}", choice)
                    }
                    other => {
                        log::error!("Error sending request: {:?}", other);
                    }
                }
            }
            DatabaseCommand::SetTell(choice) => {
                match client.send_api_request(Request::SetTell(choice)).await {
                    Ok(Response::ChoiceSet(choice)) => {
                        log::info!("Tell confirmed: {:?}", choice)
                    }
                    other => {
                        log::error!("Error sending request: {:?}", other);
                    }
                }
            }
            DatabaseCommand::Initialize(_) => unreachable!(),
        }
    }
}
