terraform {
  required_version = ">= 1.5"

  required_providers {
    github = {
      source  = "integrations/github"
      version = "~> 6.0"
    }
  }
}

provider "github" {
  owner = "horsenuggets"
  # Authenticates via the GITHUB_TOKEN environment variable.
}

locals {
  repo_name = "luau"

  # Must match the job `name:` fields in .github/workflows/ci.yml.
  main_checks = [
    "macos",
    "macos-arm",
    "ubuntu",
    "windows (Win32)",
    "windows (x64)",
  ]

  # ci.yml also runs on PRs to release, alongside .github/workflows/release-checks.yaml.
  release_checks = concat(local.main_checks, [
    "Build test - Linux",
    "Build test - macOS",
    "Build test - Windows",
    "Validate PR title",
    "Validate version",
    "Verify diff matches main",
  ])
}

# Adopt the existing repository instead of trying to create it. The id must be a literal
# string on Terraform 1.5, so keep it in sync with local.repo_name.
import {
  to = github_repository.repo
  id = "luau"
}

resource "github_repository" "repo" {
  name = local.repo_name

  has_issues   = true
  has_projects = false
  has_wiki     = false

  allow_squash_merge = true
  allow_merge_commit = false
  allow_rebase_merge = false

  squash_merge_commit_title   = "PR_TITLE"
  squash_merge_commit_message = "PR_BODY"

  delete_branch_on_merge = true

  # Managed in the GitHub UI, so applies don't overwrite them.
  lifecycle {
    ignore_changes = [description, has_downloads, homepage_url, topics, visibility]
  }
}

resource "github_repository_ruleset" "main" {
  name        = "main"
  repository  = github_repository.repo.name
  target      = "branch"
  enforcement = "active"

  conditions {
    ref_name {
      include = ["~DEFAULT_BRANCH"]
      exclude = []
    }
  }

  rules {
    non_fast_forward = true
    deletion         = true

    pull_request {
      required_approving_review_count = 1
      dismiss_stale_reviews_on_push   = true
    }

    copilot_code_review {
      review_on_push             = true
      review_draft_pull_requests = false
    }

    required_status_checks {
      strict_required_status_checks_policy = true

      dynamic "required_check" {
        for_each = local.main_checks
        content {
          context        = required_check.value
          integration_id = 0
        }
      }
    }
  }
}

resource "github_repository_ruleset" "release" {
  name        = "release"
  repository  = github_repository.repo.name
  target      = "branch"
  enforcement = "active"

  conditions {
    ref_name {
      include = ["refs/heads/release"]
      exclude = []
    }
  }

  rules {
    non_fast_forward = true
    deletion         = true

    pull_request {
      required_approving_review_count = 1
      dismiss_stale_reviews_on_push   = true
    }

    copilot_code_review {
      review_on_push             = true
      review_draft_pull_requests = false
    }

    required_status_checks {
      strict_required_status_checks_policy = true

      dynamic "required_check" {
        for_each = local.release_checks
        content {
          context        = required_check.value
          integration_id = 0
        }
      }
    }
  }
}

# The GitHub provider can't manage Copilot code review settings yet, so this calls the
# REST endpoint directly to run automatic reviews at the Lite effort level. It re-runs
# whenever the repo or the effort level changes. Lite reviews cost several times fewer AI
# credits than the Balanced default.
resource "terraform_data" "copilot_review_effort" {
  triggers_replace = [github_repository.repo.full_name, "lite"]

  provisioner "local-exec" {
    command = <<-EOT
      : "$${GITHUB_TOKEN:?GITHUB_TOKEN must be set to update Copilot code review settings}"
      curl --fail-with-body --silent --show-error -X PATCH \
        -H "Authorization: Bearer $GITHUB_TOKEN" \
        -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/${github_repository.repo.full_name}/copilot/code-review/settings" \
        -d '{"review_effort":"lite"}'
    EOT
  }
}
