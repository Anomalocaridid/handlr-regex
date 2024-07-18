use crate::{
    apps::DesktopList,
    common::{DesktopEntry, DesktopHandler},
    error::Result,
};
use derive_more::Deref;
use mime::Mime;
use std::{collections::HashMap, convert::TryFrom, ffi::OsString, io::Write};

#[derive(Debug, Default, Clone, Deref)]
pub struct SystemApps(HashMap<Mime, DesktopList>);

impl SystemApps {
    /// Get the list of handlers associated with a given mime
    // TODO: test
    pub fn get_handlers(&self, mime: &Mime) -> Option<DesktopList> {
        Some(self.get(mime)?.clone())
    }

    /// Get the primary of handler associated with a given mime
    // TODO: test
    pub fn get_handler(&self, mime: &Mime) -> Option<DesktopHandler> {
        Some(self.get_handlers(mime)?.front()?.clone())
    }

    /// Get all system-level desktop entries on the system
    #[mutants::skip] // Cannot test directly, depends on system state
    pub fn get_entries(
    ) -> Result<impl Iterator<Item = (OsString, DesktopEntry)>> {
        Ok(xdg::BaseDirectories::new()?
            .list_data_files_once("applications")
            .into_iter()
            .filter(|p| {
                p.extension().and_then(|x| x.to_str()) == Some("desktop")
            })
            .filter_map(|p| {
                Some((
                    p.file_name()?.to_owned(),
                    DesktopEntry::try_from(p.clone()).ok()?,
                ))
            }))
    }

    /// Create a new instance of `SystemApps`
    #[mutants::skip] // Cannot test directly, depends on system state
    pub fn populate() -> Result<Self> {
        let mut map = HashMap::<Mime, DesktopList>::with_capacity(50);

        Self::get_entries()?.for_each(|(_, entry)| {
            let (file_name, mimes) = (entry.file_name, entry.mime_type);
            mimes.into_iter().for_each(|mime| {
                map.entry(mime).or_default().push_back(
                    DesktopHandler::assume_valid(file_name.to_owned()),
                );
            });
        });

        Ok(Self(map))
    }

    /// List the available handlers
    // TODO: add tests
    pub fn list_handlers<W: Write>(writer: &mut W) -> Result<()> {
        Self::get_entries()?.try_for_each(|(_, e)| {
            writeln!(writer, "{}\t{}", e.file_name.to_string_lossy(), e.name)
        })?;

        Ok(())
    }
}
