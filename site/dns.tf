# The transom.sh zone. Porkbun holds the registration and delegates to
# Linode's name servers, so this zone is what answers.
#
# Run Terraform from this directory. The state lives in the Linode
# bucket transom-terraform-state. .envrc.private, which is gitignored,
# exports LINODE_TOKEN from the 1Password item "transom Linode token"
# and AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY from "transom
# Terraform state (Linode Object Storage)". The github provider reads
# GITHUB_TOKEN, such as `GITHUB_TOKEN=$(gh auth token)`.
terraform {
  required_version = ">= 1.10"

  required_providers {
    linode = {
      source  = "linode/linode"
      version = "~> 4.9"
    }
    github = {
      source  = "integrations/github"
      version = "~> 6.13"
    }
  }

  backend "s3" {
    bucket       = "transom-terraform-state"
    key          = "site.tfstate"
    region       = "us-east-1"
    use_lockfile = true

    endpoints = {
      s3 = "https://us-east-1.linodeobjects.com"
    }

    skip_credentials_validation = true
    skip_region_validation      = true
    skip_requesting_account_id  = true
    skip_metadata_api_check     = true
    skip_s3_checksum            = true
  }
}

provider "linode" {}

resource "linode_domain" "transom_sh" {
  domain    = "transom.sh"
  type      = "master"
  soa_email = "hostmaster@transom.sh"
}

# GitHub Pages serves the site (repository.tf). A zone's apex cannot
# be a CNAME, so the apex names GitHub's published Pages addresses,
# which GitHub documents under "Managing a custom domain for your
# GitHub Pages site".
resource "linode_domain_record" "apex_a" {
  for_each = toset([
    "185.199.108.153",
    "185.199.109.153",
    "185.199.110.153",
    "185.199.111.153",
  ])

  domain_id   = linode_domain.transom_sh.id
  name        = ""
  record_type = "A"
  target      = each.value
  ttl_sec     = 300
}

resource "linode_domain_record" "apex_aaaa" {
  for_each = toset([
    "2606:50c0:8000::153",
    "2606:50c0:8001::153",
    "2606:50c0:8002::153",
    "2606:50c0:8003::153",
  ])

  domain_id   = linode_domain.transom_sh.id
  name        = ""
  record_type = "AAAA"
  target      = each.value
  ttl_sec     = 300
}

# GitHub expects the www name of an apex custom domain to CNAME to the
# account's Pages hostname, and checks it before it issues the site's
# certificate. Pages redirects www to the apex.
resource "linode_domain_record" "www" {
  domain_id   = linode_domain.transom_sh.id
  name        = "www"
  record_type = "CNAME"
  target      = "chrisguidry.github.io"
  ttl_sec     = 300
}

# Let's Encrypt, for the site's certificate from GitHub Pages, and
# Google Trust Services, for the gateway's mapping on Cloud Run, may
# issue for this domain, and no other authority may.
resource "linode_domain_record" "caa" {
  domain_id   = linode_domain.transom_sh.id
  name        = ""
  record_type = "CAA"
  tag         = "issue"
  target      = "letsencrypt.org"
}

resource "linode_domain_record" "caa_google" {
  domain_id   = linode_domain.transom_sh.id
  name        = ""
  record_type = "CAA"
  tag         = "issue"
  target      = "pki.goog"
}
