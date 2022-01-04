use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Result, Message},
};
use url::Url;

pub async fn read_chat(token: String, nick: String) -> Result<()> {
    println!("Connecting to chat...");
    let (mut socket, _) = connect_async( Url::parse("wss://irc-ws.chat.twitch.tv:443").expect("Can't parse url")).await?;

    println!("Connected to chat");
    socket.send(Message::Text(format!("PASS {}", token))).await?;
    socket.send(Message::Text(format!("NICK {}", nick))).await?;
    socket.send(Message::Text("JOIN #twitch".to_string())).await?;

    while let Some(msg) = socket.next().await {
        let msg = msg?;
        if msg.is_text() {
            println!("WS: {}", msg);
        }
    }
    Ok(())
}
