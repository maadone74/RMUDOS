//! Thin wrappers around MudWorld object lifecycle.

use crate::vm::object::{Interactive, ObjectRef};
use crate::vm::value::LpcValue;
use crate::vm::MudWorld;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

pub fn boot_master(world: &MudWorld) -> Result<ObjectRef> {
    let master = world.boot_master()?;
    let _ = world.apply(master.clone(), "preload", Vec::new(), None, None);
    Ok(master)
}

pub fn connect_player(
    world: &MudWorld,
    peer: SocketAddr,
    tx: mpsc::UnboundedSender<String>,
) -> Result<ObjectRef> {
    let master = world
        .master()
        .ok_or_else(|| anyhow::anyhow!("master object not loaded"))?;

    let player = match world.apply(master, "connect", Vec::new(), None, None)? {
        LpcValue::Object(object) => object,
        _ => world.clone_object("/std/user")?,
    };

    {
        let mut guard = player.lock();
        guard.interactive = Some(Arc::new(Interactive::new(peer, "Guest", tx)));
    }

    let _ = world.apply(
        player.clone(),
        "logon",
        Vec::new(),
        Some(player.clone()),
        None,
    )?;
    Ok(player)
}

pub fn destruct_object(world: &MudWorld, object: ObjectRef) -> Result<()> {
    world.destruct_object(&object)
}
