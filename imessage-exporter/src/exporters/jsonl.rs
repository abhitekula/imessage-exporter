// File to export database as a jsonl
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde_json::to_string;

use crate::{
    app::{error::RuntimeError, progress::build_progress_bar_export, runtime::Config},
    exporters::exporter::{Exporter, Writer}, TXT,
};

use imessage_database::{
    error::table::TableError,
    tables::{
        messages::Message,
        table::{Table, ORPHANED},
    },
};

pub struct JSONL<'a> {
    /// Data that is setup from the application's runtime
    pub config: &'a Config,
    /// Handles to files we want to write messages to
    /// Map of internal unique chatroom ID to a filename
    pub files: HashMap<i32, PathBuf>,
    /// Path to file for orphaned messages
    pub orphaned: PathBuf,
}

impl<'a> Exporter<'a> for JSONL<'a> {
    fn new(config: &'a Config) -> Self {
        let mut orphaned = config.options.export_path.clone();
        orphaned.push(ORPHANED);
        orphaned.set_extension("jsonl");
        JSONL {
            config,
            files: HashMap::new(),
            orphaned,
        }
    }

    fn iter_messages(&mut self) -> Result<(), RuntimeError> {
        // Tell the user what we are doing
        eprintln!(
            "Exporting to {} as jsonl...",
            self.config.options.export_path.display()
        );

        // Keep track of current message ROWID
        let mut current_message_row = -1;

        // Set up progress bar
        let mut current_message = 0;
        let total_messages =
            Message::get_count(&self.config.db, &self.config.options.query_context)
                .map_err(RuntimeError::DatabaseError)?;
        let pb = build_progress_bar_export(total_messages);

        let mut statement =
            Message::stream_rows(&self.config.db, &self.config.options.query_context)
                .map_err(RuntimeError::DatabaseError)?;

        let messages = statement
            .query_map([], |row| Ok(Message::from_row(row)))
            .map_err(|err| RuntimeError::DatabaseError(TableError::Messages(err)))?;

        for message in messages {
            let mut msg = Message::extract(message).map_err(RuntimeError::DatabaseError)?;

            // Early escape if we try and render the same message GUID twice
            // See https://github.com/ReagentX/imessage-exporter/issues/135 for rationale
            if msg.rowid == current_message_row {
                current_message += 1;
                continue;
            }
            current_message_row = msg.rowid;

            let _ = msg.gen_text(&self.config.db);
            let message: String = to_string(&msg).unwrap() + "\n";
            TXT::write_to_file(self.get_or_create_file(&msg), &message);


            current_message += 1;
            if current_message % 99 == 0 {
                pb.set_position(current_message);
            }
        }
        pb.finish();
        Ok(())
    }

    /// Create a file for the given chat, caching it so we don't need to build it later
    fn get_or_create_file(&mut self, message: &Message) -> &Path {
        match self.config.conversation(message) {
            Some((chatroom, id)) => self.files.entry(*id).or_insert_with(|| {
                let mut path = self.config.options.export_path.clone();
                path.push(self.config.filename(chatroom));
                path.set_extension("jsonl");
                path
            }),
            None => &self.orphaned,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::
        path::PathBuf
    ;

    use crate::{
        app::attachment_manager::AttachmentManager, Config, Exporter,
        Options, JSONL,
    };
    use imessage_database::{
        util::{dirs::default_db_path, platform::Platform, query_context::QueryContext},
    };

    pub fn fake_options() -> Options {
        Options {
            db_path: default_db_path(),
            attachment_root: None,
            attachment_manager: AttachmentManager::Disabled,
            diagnostic: false,
            export_type: None,
            export_path: PathBuf::new(),
            query_context: QueryContext::default(),
            no_lazy: false,
            custom_name: None,
            use_caller_id: false,
            platform: Platform::macOS,
            ignore_disk_space: false,
        }
    }

    #[test]
    fn can_create() {
        let options = fake_options();
        let config = Config::new(options).unwrap();
        let exporter = JSONL::new(&config);
        assert_eq!(exporter.files.len(), 0);
    }
}
