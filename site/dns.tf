# The transom.sh zone. Porkbun holds the registration and delegates to
# Linode's name servers, so this zone is what answers.
#
# Run Terraform from this directory. The state lives in the Linode
# bucket transom-terraform-state. .envrc.private, which is gitignored,
# exports LINODE_TOKEN from the 1Password item "transom Linode token"
# and AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY from "transom
# Terraform state (Linode Object Storage)".
terraform {
  required_version = ">= 1.10"

  required_providers {
    linode = {
      source  = "linode/linode"
      version = "~> 4.9"
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

# The address of the cluster that serves the site. It is a reserved
# address, so it stays the same when the cluster's machine is replaced.
variable "site_ipv4" {
  description = "The public IPv4 address of the cluster that serves transom.sh"
  type        = string
  default     = "172.104.31.162"
}

resource "linode_domain" "transom_sh" {
  domain    = "transom.sh"
  type      = "master"
  soa_email = "hostmaster@transom.sh"
}

resource "linode_domain_record" "apex_a" {
  domain_id   = linode_domain.transom_sh.id
  name        = ""
  record_type = "A"
  target      = var.site_ipv4
  ttl_sec     = 300
}

# Let's Encrypt, for the site, and Google Trust Services, for the
# gateway's mapping on Cloud Run, may issue for this domain, and no other
# authority may.
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
