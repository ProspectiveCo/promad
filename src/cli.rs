// ┌───────────────────────────────────────────────────────────────────────────┐
// │                                                                           │
// │  ██████╗ ██████╗  ██████╗   Copyright (C) The Prospective Company         │
// │  ██╔══██╗██╔══██╗██╔═══██╗  All Rights Reserved - April 2022              │
// │  ██████╔╝██████╔╝██║   ██║                                                │
// │  ██╔═══╝ ██╔══██╗██║   ██║  Proprietary and confidential. Unauthorized    │
// │  ██║     ██║  ██║╚██████╔╝  copying of this file, via any medium is       │
// │  ╚═╝     ╚═╝  ╚═╝ ╚═════╝   strictly prohibited.                          │
// │                                                                           │
// └───────────────────────────────────────────────────────────────────────────┘

use crate::Migrator;

use crate::error::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use prettytable::{format, row, Table};

#[derive(Debug, Parser)]
#[clap(about = "Nomad migration tool")]
pub struct NomadCli {
    #[clap(subcommand)]
    pub subcmd: NomadSubcommand,
}

/// The subcommands of the migration CLI.
/// This can be embedded in other CLI tools so that
/// users can include migration commands in their server
/// binary, etc.
#[derive(Debug, Subcommand)]
pub enum NomadSubcommand {
    #[clap(about = "Apply migrations up to a specific migrations")]
    Apply {
        #[clap(help = "The name of the migrations to apply to (inclusive)")]
        name: Option<String>,
    },
    #[clap(about = "Revert up to a specific migrations")]
    Revert {
        #[clap(help = "The name of the migrations to revert to (inclusive)")]
        name: String,
    },
    #[clap(about = "Revert all migrations")]
    RevertAll,
    #[clap(about = "List all changes")]
    List,
}

/// Execute the subcommand given a migrator.
pub async fn interpreter<DB: sqlx::Database>(
    subcmd: NomadSubcommand,
    migrator: Migrator<DB>,
) -> Result<()> {
    match subcmd {
        NomadSubcommand::Apply { name } => match name {
            Some(name) => {
                migrator.apply_to_inclusive(&name).await?;
            }
            None => {
                migrator.apply_all().await?;
            }
        },
        NomadSubcommand::Revert { name } => {
            migrator.revert_to_inclusive(&name).await?;
        }
        NomadSubcommand::List => {
            let mut table = Table::new();
            let format = format::FormatBuilder::new()
                .column_separator('|')
                .borders(' ')
                .separators(
                    &[format::LinePosition::Title],
                    format::LineSeparator::new('-', '+', ' ', ' '),
                )
                .padding(1, 1)
                .build();
            table.set_format(format);
            table.set_titles(row!["Name", "Ran", "Run Time"]);
            migrator.list_migrations().await?.iter().for_each(|row| {
                table.add_row(row![
                    row.name.bold(),
                    if row.run_at.is_some() {
                        "✓".bold().green()
                    } else {
                        "✗".bold().dimmed()
                    },
                    row.run_at.map(|x| x.to_string()).unwrap_or_default()
                ]);
            });

            // Print the table to stdout
            table.printstd();
        }
        NomadSubcommand::RevertAll => {
            migrator.revert_all().await?;
        }
    }
    Ok(())
}
