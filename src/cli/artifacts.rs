//! Generated documentation artifacts from the augmented Clap command tree.

use super::command;
use clap::{Command, CommandFactory};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Failure while constructing or writing a generated CLI artifact.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArtifactError {
    /// The application collides with a librebar-owned command.
    #[error("cannot build the augmented CLI command: {0}")]
    Command(#[source] clap::Error),
    /// A generated artifact could not be written.
    #[error("cannot write generated CLI artifact: {0}")]
    Io(#[from] std::io::Error),
    /// Two commands resolved to the same manpage filename.
    #[error("multiple commands generated the manpage path `{0}`")]
    DuplicateManpage(PathBuf),
}

impl From<clap::Error> for ArtifactError {
    fn from(error: clap::Error) -> Self {
        Self::Command(error)
    }
}

/// Render the root command's section-1 manpage as roff.
///
/// The page uses the same augmented command tree as [`super::parse`], so its
/// subcommand list includes librebar's schema and completion commands.
///
/// # Errors
///
/// Returns an error for reserved-command collisions or writer failures.
pub fn render_manpage<T: CommandFactory>(writer: &mut dyn Write) -> Result<(), ArtifactError> {
    let mut root = command::<T>()?;
    root.build();
    let display_name = root.get_name().to_owned();
    clap_mangen::Man::new(root.display_name(display_name)).render(writer)?;
    Ok(())
}

/// Generate section-1 manpages for the root and every visible subcommand.
///
/// Filenames use full hyphen-separated command paths, such as
/// `myapp-widget-list.1`, so equally named leaves in different subtrees cannot
/// overwrite each other. The returned paths are in command-tree order.
///
/// # Errors
///
/// Returns an error for reserved-command collisions, directory/file failures,
/// or a duplicate generated filename.
pub fn generate_manpages<T: CommandFactory>(
    output_dir: impl AsRef<Path>,
) -> Result<Vec<PathBuf>, ArtifactError> {
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir)?;

    let mut root = command::<T>()?;
    root.build();
    let root_name = root.get_name().to_owned();
    let mut generated = Vec::new();
    let mut seen = BTreeSet::new();
    generate_command_manpages(root, &[root_name], output_dir, &mut seen, &mut generated)?;
    Ok(generated)
}

fn generate_command_manpages(
    command: Command,
    path: &[String],
    output_dir: &Path,
    seen: &mut BTreeSet<PathBuf>,
    generated: &mut Vec<PathBuf>,
) -> Result<(), ArtifactError> {
    let subcommands: Vec<_> = command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
        .cloned()
        .collect();
    let display_name = path.join("-");
    let page = clap_mangen::Man::new(command.display_name(display_name));
    let output_path = output_dir.join(page.get_filename());
    if !seen.insert(output_path.clone()) {
        return Err(ArtifactError::DuplicateManpage(output_path));
    }

    let mut file = std::fs::File::create(&output_path)?;
    page.render(&mut file)?;
    file.flush()?;
    generated.push(output_path);

    for subcommand in subcommands {
        let mut subcommand_path = path.to_vec();
        subcommand_path.push(subcommand.get_name().to_owned());
        generate_command_manpages(subcommand, &subcommand_path, output_dir, seen, generated)?;
    }
    Ok(())
}
