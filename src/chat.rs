use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Result, Message},
};
use ringbuf::Producer;
use url::Url;
use regex::Regex;

pub struct ChatMessage {
    pub sender: String,
    pub message: String,
}

impl ChatMessage {
    fn parse(s: String) -> Option<ChatMessage> {
        let re = Regex::new(r":([^:]+)![^:]+:(.+)").unwrap();
        let cap = match re.captures(&s) {
            None => { return None; }
            Some(c) => c,
        };

        Some(Self {
            sender: cap[1].to_string(),
            message: cap[2].to_string(),
        })
    }

    fn string(&self) -> String {
        format!("{}: {}", self.sender, self.message)
    }
}


pub async fn read_chat(token: String, nick: String, mut prod: Producer<String>) -> Result<()> {
    println!("Connecting to chat...");
    let (mut socket, _) = connect_async( Url::parse("wss://irc-ws.chat.twitch.tv:443").expect("Can't parse url")).await?;

    println!("Connected to chat");
    socket.send(Message::Text(format!("PASS {}", token))).await?;
    socket.send(Message::Text(format!("NICK {}", nick))).await?;
    socket.send(Message::Text("JOIN #bnans".to_string())).await?;

    while let Some(msg) = socket.next().await {
        let msg = msg?;
        if msg.is_text() {
            for payload in msg.into_text().unwrap().split("\r\n") {
                if payload.len() == 0 { continue } 

                // TODO: match PING with PONG

                let m = match ChatMessage::parse(payload.to_string()) {
                    Some(m) => m,
                    None => { continue },
                };
                match prod.push(m.string()) {
                    Ok(_) => {},
                    Err(e) => { println!("Error writing to buffer: {}", e); }
                }
            }
        }
    }
    Ok(())
}
