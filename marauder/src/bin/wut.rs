#![feature(nll)]

#[macro_use]
extern crate log;
extern crate env_logger;

use docopt::Docopt;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::delay_for;
use tonic::transport::Endpoint;
use tonic::Request;
use whiteboard::whiteboard_client::WhiteboardClient;
use whiteboard::{Event, RecvEventsReq, SendEventReq};

pub mod whiteboard {
    tonic::include_proto!("hypercard.whiteboard");
}

fn add_xuser<T>(req: &mut Request<T>, user: String) {
    let user_id = user.parse().unwrap();
    let md = Request::metadata_mut(req);
    assert!(md.insert("x-user", user_id).is_none());
}

const USAGE: &str = "
reMarkable whiteboard HyperCard.

Usage:
  whiteboard [--host=<HOST>] [--user=USER] [--room=<ROOM>]
  whiteboard (-h | --help)
  whiteboard --version

Options:
  --host=<HOST>  Server to connect to [default: http://fknwkdacd.com:10000].
  --user=<USER>  User to connect as [default: Jane].
  --room=<ROOM>  Room to join [default: living-room].
  -h --help      Show this screen.
  --version      Show version.
";

#[derive(Debug, Deserialize, Clone)]
struct Args {
    flag_host: String,
    flag_user: String,
    flag_room: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());
    debug!("{:?}", args);

    let host = args.clone().flag_host;
    info!("[main] connecting to {:?}...", host);

    let channel = Endpoint::from_shared(host).unwrap().connect().await?;
    let mut client1 = WhiteboardClient::new(channel.clone());
    let mut client2 = WhiteboardClient::new(channel);

    let args2 = args.clone();
    info!("[loop_recv] spawn-ing");
    tokio::spawn(async move {
        info!("[loop_recv] spawn-ed");
        let mut req = Request::new(RecvEventsReq {
            room_id: args.flag_room,
        });
        add_xuser(&mut req, args.flag_user);
        info!("[loop_recv] creating stream");
        let mut stream = client2.recv_events(req).await.unwrap().into_inner();
        info!("[loop_recv] receiving...");
        while let Some(event) = stream.message().await.unwrap() {
            debug!("[loop_recv] received event {:?}", event);
            // tokio::task::yield_now().await;
        }
        info!("[loop_recv] terminated");
    });

    info!("[TXer] spawn-ing");
    tokio::spawn(async move {
        info!("[TXer] spawn-ed");
        loop {
            delay_for(Duration::from_millis(500)).await;

            let mut req = Request::new(SendEventReq {
                event: Some(Event {
                    created_at: 0,
                    user_id: "".into(),
                    room_id: "".into(),
                    event_drawing: None,
                    event_user_left_the_room: false,
                    event_user_joined_the_room: true,
                }),
                room_ids: vec![args2.flag_room.to_owned()],
            });
            add_xuser(&mut req, "Bob".to_owned());
            info!("[TXer] req: {:?}", req);
            let rep = client1
                .send_event(req)
                .await
                .map_err(|e| error!("[TXer] !Send: {:?}", e));
            info!("[TXer] rep: {:?}", rep);
            // tokio::task::yield_now().await;
        }
        // info!("[TXer] terminated");
    });

    delay_for(Duration::from_secs(10)).await;
    Ok(())
}
