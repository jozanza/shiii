use std::collections::VecDeque;
use turbo::*;

#[turbo::game]
struct GameState {
    screen: Screen,
    chat_log: VecDeque<chat::UserEvent>,
}
impl GameState {
    pub fn new() -> Self {
        Self {
            screen: Screen::default(),
            chat_log: VecDeque::new(),
        }
    }
    pub fn update(&mut self) {
        if gamepad::get(0).select.just_pressed() {
            if self.screen == Screen::Chat {
                self.chat_log.clear();
            }
            self.screen = self.screen.next();
        }

        match self.screen {
            Screen::Counter => self.update_counter(),
            Screen::Hello => self.update_hello(),
            Screen::Chat => self.update_chat(),
        }
    }
    fn update_hello(&mut self) {
        text!("HELLO PROGRAM — PRESS START");
        if gamepad::get(0).start.just_pressed() {
            hello::Greet.exec();
        }
    }
    fn update_counter(&mut self) {
        let p = mouse::screen();
        let size = if p.pressed() { 16 } else { 8 };
        circ!(
            d = size,
            x = p.x - (size / 2),
            y = p.y - (size / 2),
            color = 0xff0000ff
        );

        let amount = 100;

        if p.left.just_pressed() {
            let cmd = counter::AddCommand::Plus(amount);
            cmd.exec();
        }
        if p.right.just_pressed() {
            let cmd = counter::AddCommand::Minus(amount);
            cmd.exec();
        }
        if gamepad::get(0).start.just_pressed() {
            counter::ResetCommand.exec();
        }

        if let Some(counter) = counter::watch::<Counter>("counter") {
            text!("{:?}", counter);
        }
    }
    fn update_chat(&mut self) {
        text!("CHAT PROGRAM");
        if let Some(conn) = chat::MainChannel::subscribe() {
            while let Ok(Some(user_event)) = conn.recv() {
                if self.chat_log.len() >= 14 {
                    self.chat_log.pop_front();
                }
                self.chat_log.push_back(user_event);
            }
            if gamepad::get(0).start.just_pressed() {
                let _ = conn.send(&chat::UserMessage::Emote(chat::Emote::Love));
            }
        }
        for (i, msg) in self.chat_log.iter().rev().enumerate() {
            text!("{:?}", msg; x = 4, y = 8 + i as i32 * 10);
        }
    }
}

#[turbo::serialize]
#[derive(Default, Copy, PartialEq)]
enum Screen {
    #[default]
    Hello,
    Counter,
    Chat,
}
impl Screen {
    fn next(self) -> Self {
        match self {
            Screen::Hello => Screen::Counter,
            Screen::Counter => Screen::Chat,
            Screen::Chat => Screen::Hello,
        }
    }
}

#[turbo::serialize]
struct Counter {
    value: i32,
}

#[turbo::program]
pub mod hello {
    use super::*;

    #[turbo::command(name = "greet")]
    pub struct Greet;
    impl CommandHandler for Greet {
        fn run(&mut self, user_id: &str) -> Result<(), std::io::Error> {
            use turbo::os::server;
            server::log!("Hey, {user_id}!");
            Ok(())
        }
    }
}

#[turbo::program]
pub mod counter {
    use super::*;

    #[turbo::command(name = "add")]
    pub enum AddCommand {
        Plus(i32),
        Minus(i32),
    }
    impl AddCommand {
        pub fn amount(&self) -> i32 {
            match self {
                Self::Minus(n) => -*n,
                Self::Plus(n) => *n,
            }
        }
    }
    impl CommandHandler for AddCommand {
        fn run(&mut self, user_id: &str) -> Result<(), std::io::Error> {
            use turbo::os::server;
            server::log!("{user_id}, {self:?}");
            let mut counter = server::fs::read("counter").unwrap_or(Counter { value: 0 });
            counter.value += self.amount();
            server::log!("Incremented = {:?}", counter);
            server::fs::write("counter", &counter)?;
            Ok(())
        }
    }

    #[turbo::command(name = "reset")]
    pub struct ResetCommand;
    impl CommandHandler for ResetCommand {
        fn run(&mut self, user_id: &str) -> Result<(), std::io::Error> {
            use turbo::os::server;
            if user_id != PROGRAM_OWNER {
                server::bail!("YOU NOT DA OWNER!");
            }
            let counter = Counter { value: 0 };
            server::fs::write("counter", &counter)?;
            Ok(())
        }
    }
}

#[turbo::program]
pub mod chat {
    use turbo::os::server::channel::ChannelSettings;

    use super::*;
    use std::collections::BTreeMap;

    #[turbo::serialize]
    pub enum UserMessage {
        Emote(Emote),
        Move(f32, f32),
    }

    #[turbo::serialize]
    pub enum Emote {
        Love,
        Anger,
        Sob,
        Thinking,
    }

    #[turbo::serialize]
    pub enum UserEvent {
        Move {
            user_id: String,
            position: (f32, f32),
        },
        Emote {
            user_id: String,
            kind: Emote,
        },
        Enter {
            user_id: String,
        },
        Leave {
            user_id: String,
        },
        Tick,
    }

    #[turbo::channel(name = "main")]
    pub struct MainChannel {
        positions: BTreeMap<String, (f32, f32)>,
    }
    impl ChannelHandler for MainChannel {
        type Send = UserEvent;
        type Recv = UserMessage;

        fn new() -> Self {
            Self {
                positions: BTreeMap::new(),
            }
        }

        fn on_open(&mut self, settings: &mut ChannelSettings) {
            settings.set_interval(16 * 60 * 10);
        }

        fn on_interval(&mut self) {
            os::server::channel::broadcast(Self::Send::Tick);
        }

        fn on_connect(&mut self, user_id: &str) {
            os::server::log!("{user_id} connected");
            os::server::channel::broadcast(Self::Send::Enter {
                user_id: user_id.to_string(),
            });
        }

        fn on_data(&mut self, user_id: &str, data: Self::Recv) {
            match data {
                UserMessage::Move(dx, dy) => {
                    let pos = self.positions.entry(user_id.to_string()).or_default();
                    pos.0 += dx;
                    pos.1 += dy;
                    os::server::channel::broadcast(Self::Send::Move {
                        user_id: user_id.to_string(),
                        position: *pos,
                    });
                }
                UserMessage::Emote(kind) => {
                    os::server::channel::broadcast(Self::Send::Emote {
                        user_id: user_id.to_string(),
                        kind,
                    });
                }
            }
        }

        fn on_disconnect(&mut self, user_id: &str) {
            os::server::channel::broadcast(Self::Send::Leave {
                user_id: user_id.to_string(),
            });
        }
    }
}
