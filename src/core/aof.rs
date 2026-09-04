use std::{
    fs::{self, File, OpenOptions},
    io::{Error, Write},
    path::Path,
    time::{Duration, Instant},
};

use log::info;

use crate::{
    config::config,
    core::{
        resp::{self},
        store::{Obj, Store},
    },
};

pub struct AofConfig {
    pub flush_freq_sec: Duration,
    pub last_flush_time: Instant,
}

impl AofConfig {
    pub fn new() -> Self {
        AofConfig {
            flush_freq_sec: Duration::from_secs(1),
            last_flush_time: Instant::now(),
        }
    }
}

pub struct Aof {
    file: File,
    size_bytes: u64,
}

impl Aof {
    fn new(file: File) -> Result<Self, Error> {
        let Ok(file_metadata) = file.metadata() else {
            return Err(Error::other("Err accessing file metadata".to_string()));
        };

        Ok(Aof {
            size_bytes: file_metadata.len(),
            file,
        })
    }

    pub fn open_or_create() -> Result<Aof, Error> {
        let aof_file_path = Path::new(&config().aof_file_name);
        let file = match File::create_new(aof_file_path) {
            Ok(file) => file,
            Err(e) => {
                let Ok(file) = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(aof_file_path)
                else {
                    return Err(Error::other(format!("Err opening AOF file: {e:?}")));
                };
                file
            }
        };

        Aof::new(file)
    }

    pub fn flush(&self) -> Result<(), Error> {
        self.file.sync_all()
    }

    pub fn append(&mut self, data: Vec<u8>) -> Result<(), Error> {
        self.file.write_all(&data)
    }

    pub fn dump_all_aof(&mut self, store: &Store) -> Result<(), Error> {
        info!("Rewriting AOF file");

        let aof_path = Path::new(&config().aof_file_name);
        let tmp_aof_name = config().aof_file_name.clone() + ".tmp";
        let tmp_aof_path = Path::new(&tmp_aof_name);

        let write_result = (|| -> Result<File, Error> {
            let mut file = File::create(tmp_aof_path)?;

            for (k, obj) in store.iter() {
                self.dump_key(&mut file, k, obj)?;
            }

            file.sync_all()?;
            Ok(file)
        })();

        match write_result {
            Ok(file) => {
                if aof_path.exists() {
                    fs::remove_file(aof_path)?;
                }
                fs::rename(tmp_aof_path, aof_path)?;

                *self = Aof::new(file)?;

                info!("AOF file rewrite completed");
                Ok(())
            }
            Err(e) => {
                let _ = fs::remove_file(tmp_aof_path);
                Err(e)
            }
        }
    }

    fn dump_key(&self, file: &mut File, key: &String, obj: &Obj) -> Result<usize, Error> {
        let mut cmd_vec = vec!["SET".to_string(), key.to_owned(), obj.val.to_owned()];
        if let Some(expires_at) = &obj.expires_at {
            cmd_vec.push("PEXPIREAT".to_string());
            cmd_vec.push(expires_at.to_string());
        }

        let encoded_cmd = resp::encode_cmd(cmd_vec)
            .map_err(|e| Error::other(format!("Err encoding command: {e:?}")))?;

        file.write(&encoded_cmd)
    }
}
