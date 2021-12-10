use std::time::Duration;

use bonsaidb::client::Client;
use gooey::{
    core::{Context, StyledWidget},
    widgets::{
        button::Button,
        component::{Behavior, Component, ComponentCommand, Content, EventMapper},
        label::Label,
        layout::{Dimension, Layout, WidgetLayout},
    },
    App,
};
use minority_game_shared::{Api, Choice, Request, Response};

fn main() {
    // The user interface and database will be run separately, and flume
    // channels will send `DatabaseCommand`s to do operations on the database
    // server.
    let (command_sender, command_receiver) = flume::unbounded();

    // Spawn an async task that processes commands sent by `command_sender`.
    App::spawn(process_database_commands(command_receiver));

    App::from_root(|storage|
        // The root widget is a `Component` with our component behavior
        // `Counter`.
        Component::new(GameInterface::new(command_sender), storage))
    // Register our custom component's transmogrifier.
    .with_component::<GameInterface>()
    // Run the app using the widget returned by the initializer.
    .run()
}

/// The state of the `Counter` component.
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
    type Widgets = CounterWidgets;

    fn build_content(
        &mut self,
        builder: <Self::Content as Content<Self>>::Builder,
        events: &EventMapper<Self>,
    ) -> StyledWidget<Layout> {
        builder
            .with(
                CounterWidgets::GoOut,
                Button::new("Go Out", events.map(|_| GameInterfaceEvent::GoOutClicked)),
                WidgetLayout::build()
                    .left(Dimension::zero())
                    .top(Dimension::zero())
                    .finish(),
            )
            .with(
                CounterWidgets::StayIn,
                Button::new("Stay In", events.map(|_| GameInterfaceEvent::StayInClicked)),
                WidgetLayout::build()
                    .top(Dimension::zero())
                    .right(Dimension::zero())
                    .finish(),
            )
            .with(
                CounterWidgets::Status,
                Label::new("Connecting..."),
                WidgetLayout::build()
                    .bottom(Dimension::zero())
                    .left(Dimension::zero())
                    .right(Dimension::zero())
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
            GameInterfaceEvent::GoOutClicked => {
                let _ = component
                    .behavior
                    .command_sender
                    .send(DatabaseCommand::SetChoice(Choice::GoOut));
            }
            GameInterfaceEvent::StayInClicked => {
                let _ = component
                    .behavior
                    .command_sender
                    .send(DatabaseCommand::SetChoice(Choice::StayIn));
            }
            GameInterfaceEvent::UpdateStatus(status) => {
                let label = component
                    .widget_state(&CounterWidgets::Status, context)
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
enum CounterWidgets {
    GoOut,
    StayIn,
    Status,
}

#[derive(Debug)]
enum GameInterfaceEvent {
    GoOutClicked,
    StayInClicked,
    UpdateStatus(String),
}

/// Commands that the user interface will send to the database task.
enum DatabaseCommand {
    /// Initializes the worker with a context, which
    Initialize(DatabaseContext),
    /// Increment the counter.
    SetChoice(Choice),
}

/// A context provides the information necessary to communicate with the user
/// inteface.
#[derive(Clone)]
struct DatabaseContext {
    /// The context of the component.
    context: Context<Component<GameInterface>>,
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
        match Client::build("ws://127.0.0.1:8081".parse().unwrap())
            .with_custom_api_callback::<Api, _>(move |response| match response {
                Ok(Response::Welcome {
                    happiness,
                    player_id
                }) => {
                    let _ = api_callback_context.context.send_command(ComponentCommand::Behavior(GameInterfaceEvent::UpdateStatus(
                        format!("Welcome {}! Current happiness: {}",
                            player_id,
                            (happiness * 100.) as u32,
                        )
                    )));
                }
                Ok(Response::ChoiceSet(_)) => unreachable!(),
                Ok(Response::RoundComplete {
                    won,
                    happiness,
                    current_rank,
                    number_of_players,
                }) => {
                    let _ = api_callback_context.context.send_command(ComponentCommand::Behavior(GameInterfaceEvent::UpdateStatus(
                        format!("You {}! Current happiness: {}%. Ranked {} of {} players in the last round.",
                            if won {
                                "won"
                            } else {
                                "lost"
                            },
                            (happiness * 100.) as u32,
                            current_rank,
                            number_of_players
                        )
                    )));
                }
                Ok(Response::RoundPending {
                    current_rank,
                    number_of_players,
                    seconds_remaining
                }) => {
                    let _ = api_callback_context.context.send_command(ComponentCommand::Behavior(GameInterfaceEvent::UpdateStatus(
                        format!("Round starting in {} seconds! Ranked {} of {} players currently playing.",
                            seconds_remaining,
                            current_rank,
                            number_of_players
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
            DatabaseCommand::Initialize(_) => unreachable!(),
        }
    }
}
