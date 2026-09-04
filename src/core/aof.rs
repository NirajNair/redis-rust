use std::{
    fs::File,
    io::{Error, Write},
    path::Path,
};

use log::info;

use crate::{
    config::config,
    core::{
        resp::{self},
        store::{Obj, Store},
    },
};

pub fn dump_all_aof(store: &Store) -> Result<(), Error> {
    let aof_file_path = Path::new(&config().aof_file_name);
    let mut file = File::create_new(aof_file_path)?;

    info!("rewriting AOF file at {}", aof_file_path.display());

    for (k, obj) in store.iter() {
        dump_key(&mut file, k, obj)?;
    }

    info!("AOF file rewrite completed");
    Ok(())
}

fn dump_key(file: &mut File, key: &String, obj: &Obj) -> Result<usize, Error> {
    let cmd = vec!["SET".to_string(), key.to_owned(), obj.val.to_owned()];

    let encoded_cmd =
        resp::encode_cmd(cmd).map_err(|e| Error::other(format!("Err encoding command: {e:?}")))?;

    file.write(&encoded_cmd)
}
