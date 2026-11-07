#!/usr/bin/env bash
# verify-manifest-children.sh — assert a published manifest index was assembled
# out of the images this build job pushed.
#
# The merge job reads its manifest-list digest back from a tag, and ttl.sh has
# no authentication while the run id in that tag is public for as long as the
# run is. In the seconds between the manifest push and the read-back, anyone can
# repoint that tag. The digest read back would then be theirs, and every check
# downstream would accept it, because it really is a well-formed content address
# to a real image. It is just not this build.
#
# What `/tmp/digests` names is not what the published index lists. Provenance is
# on, so each per-arch build pushes an OCI index of its own -- a platform
# manifest plus an attestation manifest -- and `imagetools create` flattens
# index sources, so the published index carries those children and never the
# source digests. Each pushed digest is therefore resolved to its own index, and
# it is that index's children that are compared.
#
# The comparison is equality, not containment. Nothing here constrains the
# `platform` field, and a runtime pulls the image by platform, so
# an index that keeps all of this job's manifests, relabels the amd64 one and
# puts a foreign manifest in the linux/amd64 slot would satisfy a subset test
# and still hand over a substituted image. Requiring the published set to hold
# nothing else is what closes that. `imagetools create` adds nothing of its own
# to a flattened index -- a real run publishes exactly the union of its sources'
# children -- so equality is what the merge job actually produces.
#
# Usage: verify-manifest-children.sh <image> <published-index-json> <digests-dir>

set -euo pipefail

die() { echo "ERROR: $*" >&2; exit 1; }

[[ $# -eq 3 ]] || die "usage: $0 <image> <published-index-json> <digests-dir>"

image="$1"
published="$2"
digests_dir="$3"

[[ -f "${published}" ]] || die "manifest index ${published} does not exist"
[[ -d "${digests_dir}" ]] || die "digests directory ${digests_dir} does not exist"

expected=""
for digest_file in "${digests_dir}"/*; do
  [[ -f "${digest_file}" ]] || continue
  source_digest="sha256:${digest_file##*/}"

  raw="$(docker buildx imagetools inspect "${image}@${source_digest}" --raw)" \
    || die "cannot read ${source_digest}, which this job pushed"

  # With provenance off a build pushes a plain manifest instead of an index. It
  # has no children, and then the source digest is itself what must appear.
  children="$(jq --raw-output '.manifests[]?.digest' <<< "${raw}")"
  [[ -n "${children}" ]] || children="${source_digest}"
  expected+="${children}"$'\n'
done

# Without this an empty directory would leave both sets empty and equal, and the
# check would wave through any index at all.
[[ -n "${expected//[[:space:]]/}" ]] \
  || die "no pushed digests to check in ${digests_dir}"

expected="$(grep --invert-match '^$' <<< "${expected}" | LC_ALL=C sort --unique)"
found="$(jq --raw-output '.manifests[]?.digest' "${published}" | LC_ALL=C sort --unique)"

missing="$(grep --fixed-strings --line-regexp --invert-match \
  --file=<(printf '%s\n' "${found}") <<< "${expected}" || true)"
[[ -z "${missing}" ]] \
  || die "the published index does not list $(tr '\n' ' ' <<< "${missing}")-- pushed by this job"

extra="$(grep --fixed-strings --line-regexp --invert-match \
  --file=<(printf '%s\n' "${expected}") <<< "${found}" || true)"
[[ -z "${extra}" ]] \
  || die "the published index also lists $(tr '\n' ' ' <<< "${extra}")-- not pushed by this job"

echo "published index matches the $(wc -l <<< "${expected}" | tr -d ' ') manifests this job pushed"
