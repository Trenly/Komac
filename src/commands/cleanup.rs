use crate::credential::handle_token;
use crate::github::github_client::GitHub;
use crate::github::graphql::get_pull_request_from_branch::PullRequestState;
use anstream::println;
use clap::Parser;
use color_eyre::Result;
use futures_util::{stream, StreamExt};
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::MultiSelect;
use owo_colors::OwoColorize;
use std::num::NonZeroUsize;
use std::time::Duration;

/// Finds branches from the fork of winget-pkgs that have had a merged or closed pull request to microsoft/winget-pkgs
/// from them, prompting for which ones to delete
#[derive(Parser)]
pub struct Cleanup {
    /// Only delete merged branches
    #[arg(long)]
    only_merged: bool,

    /// Only delete closed branches
    #[arg(long)]
    only_closed: bool,

    /// Automatically delete all relevant branches
    #[arg(short, long, env = "CI")]
    all: bool,

    /// Number of calls to send to GitHub concurrently
    #[arg(short, long, default_value_t = NonZeroUsize::new(num_cpus::get()).unwrap())]
    concurrent_calls: NonZeroUsize,

    /// GitHub personal access token with the `public_repo` scope
    #[arg(short, long, env = "GITHUB_TOKEN")]
    token: Option<String>,
}

impl Cleanup {
    pub async fn run(self) -> Result<()> {
        let token = handle_token(self.token).await?;
        let github = GitHub::new(&token)?;

        // Get all winget-pkgs branches from the user's fork except the default one
        let (branches, repository_id, default_branch) =
            github.get_branches(&github.get_username().await?).await?;

        let merge_state = match (self.only_merged, self.only_closed) {
            (true, false) => "merged",
            (false, true) => "closed",
            _ => "merged or closed",
        };

        let pb_style = ProgressStyle::with_template(
            "{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len}",
        )
        .unwrap();

        // Retrieve an associated pull request for each branch
        let pb = ProgressBar::new(branches.len() as u64)
            .with_style(pb_style)
            .with_message(format!(
                "Retrieving branches that have a {merge_state} pull request associated with them"
            ));
        let pull_requests = stream::iter(branches.iter())
            .map(|branch| async {
                pb.inc(1);
                if let Ok(Some(pull_request)) = github
                    .get_pull_request_from_branch(&default_branch, &branch.name)
                    .await
                {
                    Some((pull_request, branch.name.as_str()))
                } else {
                    None
                }
            })
            .buffered(self.concurrent_calls.get())
            .filter_map(|opt| async {
                // Filter to only pull requests that have a branch associated with them and match
                // the merged/closed state requirement
                opt.filter(
                    |(pull_request, _)| match (self.only_merged, self.only_closed) {
                        (true, false) => pull_request.state == PullRequestState::Merged,
                        (false, true) => pull_request.state == PullRequestState::Closed,
                        _ => matches!(
                            pull_request.state,
                            PullRequestState::Merged | PullRequestState::Closed
                        ),
                    },
                )
            })
            .collect::<IndexMap<_, _>>()
            .await;
        pb.finish_and_clear();

        // Exit if there are no branches to delete
        if pull_requests.is_empty() {
            println!(
                "There are no {} pull requests with branches that can be deleted",
                merge_state.blue()
            );
            return Ok(());
        }

        let chosen_pr_branches = if self.all {
            pull_requests.keys().collect()
        } else {
            // Show a multi-selection prompt for which branches to delete, with all options pre-selected
            MultiSelect::new(
                "Please select branches to delete",
                pull_requests.keys().collect(),
            )
            .with_all_selected_by_default()
            .with_page_size(10)
            .prompt()?
        };

        let branches_to_delete = chosen_pr_branches
            .into_iter()
            .filter_map(|pull_request| pull_requests.get(pull_request).copied())
            .collect::<Vec<_>>();

        // Delete all selected branches
        let pb = ProgressBar::new_spinner().with_message("Deleting selected branches");
        pb.enable_steady_tick(Duration::from_millis(50));

        github
            .delete_branches(&repository_id, branches_to_delete)
            .await?;

        pb.finish_and_clear();

        Ok(())
    }
}
