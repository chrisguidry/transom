# The chrisguidry/transom repository's public face and its Pages site.
# The zone's records for the site are in dns.tf, so the DNS half and the
# Pages half of transom.sh read from the same directory.
#
# The repository and its Pages site predate this file, so the import
# blocks below adopted them into the state. Terraform never creates or
# deletes the repository.

provider "github" {
  owner = "chrisguidry"
}

import {
  to = github_repository.transom
  id = "transom"
}

resource "github_repository" "transom" {
  name         = "transom"
  description  = "Ask a person a question from a program, and get the answer back"
  homepage_url = "https://transom.sh"
  visibility   = "public"
  topics       = ["human-in-the-loop", "notifications", "mcp", "webhooks", "rust", "android"]

  # Stated so an apply never writes a provider default over a value
  # someone chose.
  has_issues                  = true
  has_projects                = false
  has_wiki                    = false
  has_discussions             = false
  allow_merge_commit          = false
  allow_squash_merge          = true
  allow_rebase_merge          = true
  allow_auto_merge            = true
  allow_update_branch         = true
  delete_branch_on_merge      = true
  web_commit_signoff_required = false
  squash_merge_commit_title   = "PR_TITLE"
  squash_merge_commit_message = "PR_BODY"

  lifecycle {
    prevent_destroy = true
    # Dependabot alert state is the repository's history, not its
    # public face, so this file does not manage it.
    ignore_changes = [vulnerability_alerts]
  }
}

import {
  to = github_repository_pages.site
  id = "transom"
}

# A workflow deploy (.github/workflows/site.yaml), so no branch source,
# and the redirect from http. The provider deprecates the inline pages
# block on github_repository, which cannot state https_enforced.
resource "github_repository_pages" "site" {
  repository     = github_repository.transom.name
  build_type     = "workflow"
  cname          = "transom.sh"
  https_enforced = true
}
