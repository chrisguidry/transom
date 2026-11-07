# transom.sh's Pages site. The zone's records for the site are in
# dns.tf, so the DNS half and the Pages half of transom.sh read from the
# same directory. The repository itself, with its merge settings and
# rulesets, belongs to chrisguidry/.github, which manages every
# repository under the account and leaves each one's Pages site to it.

provider "github" {
  owner = "chrisguidry"
}

# chrisguidry/.github holds the repository, so this state lets go of it
# without touching it.
removed {
  from = github_repository.transom

  lifecycle {
    destroy = false
  }
}

# A workflow deploy (.github/workflows/site.yaml), so no branch source,
# and the redirect from http. The provider deprecates the inline pages
# block on github_repository, which cannot state https_enforced.
#
# An update that changes https_enforced has also cleared the custom
# domain, so after any apply that changes this resource, check that
# transom.sh still serves.
resource "github_repository_pages" "site" {
  repository     = "transom"
  build_type     = "workflow"
  cname          = "transom.sh"
  https_enforced = true
}
