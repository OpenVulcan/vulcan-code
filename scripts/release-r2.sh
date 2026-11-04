#!/bin/sh
# R2 and CDN steps shared by the release, promote and setup release workflows.
# 发布、推进与安装器发布工作流共用的 R2 与 CDN 步骤。
#
# Environment: UPDATE_BASE_URL (https://<host>/<prefix>, whose path is also the object key prefix; the
# workflows take it from `vulcan-release-tool origin`), R2_ENDPOINT, R2_BUCKET, the AWS_* credentials,
# CLOUDFLARE_ZONE_ID and CLOUDFLARE_API_TOKEN.
# Objects below <objects> use the layout of vulcan-install-core `layout.rs`.
# 环境变量：UPDATE_BASE_URL（https://<主机>/<前缀>，其路径同时是对象键前缀；工作流取自
# `vulcan-release-tool origin`）、R2_ENDPOINT、R2_BUCKET、AWS_* 凭据、CLOUDFLARE_ZONE_ID 与 CLOUDFLARE_API_TOKEN。
# <objects> 之下的对象采用 vulcan-install-core `layout.rs` 的布局。
#
#   release-r2.sh unpublished <version> <platform>...   fail when any manifest of the version exists
#   release-r2.sh upload <objects>                      upload every immutable object, manifests last
#   release-r2.sh pointers <objects> <channel>          upload the channel's pointers, purge and read back
#   release-r2.sh manifests <objects> <version> <platform>...   download signed manifests for new pointers
#   release-r2.sh records <artifacts>                   upload the reuse records of the build artifacts and purge
#   release-r2.sh setup-unpublished <version>           fail when the setup version is already published
#   release-r2.sh setup-upload <objects> <version>      upload the immutable files of a setup version
#   release-r2.sh setup-pointer <objects> <channel>     upload the channel's setup pointer, purge and read back
#   release-r2.sh setup-pointer-get <objects> <channel> download the channel's setup pointer for a promotion
#   release-r2.sh setup-scripts <objects> <channel>     serve the scripts the setup pointer names at the top level
set -eu

case "${UPDATE_BASE_URL:-}" in https://*/*) ;; *) echo 'UPDATE_BASE_URL must be https://<host>/<prefix>' >&2; exit 1 ;; esac
base=${UPDATE_BASE_URL%/}
prefix=${base#https://*/}

# s3 runs one AWS CLI s3api call against R2.
# s3 对 R2 执行一次 AWS CLI s3api 调用。
s3() { aws s3api "$@" --endpoint-url "$R2_ENDPOINT" --bucket "$R2_BUCKET"; }

# put_pointers uploads the pointers at the given keys below <objects> with their signatures, purges them and
# reads each back through the CDN.
# put_pointers 上传 <objects> 之下给定键的指针及其签名，清除 CDN 缓存，并经 CDN 逐个读回。
put_pointers() {
  objects=$1
  shift
  [ "$#" -gt 0 ] || { echo "no pointers below $objects" >&2; exit 1; }
  urls=''
  for name in "$@"; do
    # Pointers change in place: a short max-age, and the purge below makes the change visible at once.
    # 指针原地更新：缓存时间短，并由下面的清除使变化立即可见。
    s3 put-object --key "$prefix/$name.sig" --body "$objects/$name.sig" --content-type application/octet-stream \
      --cache-control 'public, max-age=60' > /dev/null
    s3 put-object --key "$prefix/$name" --body "$objects/$name" --content-type application/json \
      --cache-control 'public, max-age=60' > /dev/null
    urls="$urls $base/$name $base/$name.sig"
  done
  # shellcheck disable=SC2086 # urls is a space-separated list of URLs without spaces.
  purge $urls
  # A purge propagates within seconds; each pointer must read back through the CDN unchanged.
  # 清除在数秒内生效；每个指针都必须经 CDN 原样读回。
  for name in "$@"; do
    seen=''
    for attempt in 1 2 3 4 5 6 7 8 9 10; do
      if curl -fsS "$base/$name" | cmp -s - "$objects/$name" &&
         curl -fsS "$base/$name.sig" | cmp -s - "$objects/$name.sig"; then seen=yes; break; fi
      sleep 2
    done
    [ "$seen" = yes ] || { echo "the CDN does not serve the new $name" >&2; exit 1; }
    echo "$name is live"
  done
}

# purge drops the given public URLs from the CDN cache.
# purge 从 CDN 缓存中清除给定的公开网址。
purge() {
  jq -n '{files: $ARGS.positional}' --args "$@" |
    curl -fsS -X POST "https://api.cloudflare.com/client/v4/zones/$CLOUDFLARE_ZONE_ID/purge_cache" \
      -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" -H 'Content-Type: application/json' --data @- |
    jq -e '.success == true' > /dev/null
}

command=$1
shift
case "$command" in
  unpublished)
    version=$1
    shift
    for platform in "$@"; do
      key="$prefix/releases/$version/$platform/manifest.json"
      # Published objects are immutable and cached for a year, so a version is never published twice.
      # 已发布对象不可变且缓存一年，因此同一版本绝不发布两次。
      if s3 head-object --key "$key" > /dev/null 2>&1; then
        echo "$version is already published for $platform ($key)" >&2
        exit 1
      fi
    done
    echo "$version is not published yet"
    ;;
  upload)
    objects=$1
    # Content-addressed objects first, then the manifests that reference them, so a manifest is never
    # visible before its objects; an object already in the bucket with the same size is skipped.
    # 先上传按内容寻址的对象，再上传引用它们的版本清单，使版本清单不会先于其对象可见；桶中已有且大小相同的对象跳过。
    immutable='public, max-age=31536000, immutable'
    aws s3 sync "$objects/" "s3://$R2_BUCKET/$prefix/" --endpoint-url "$R2_ENDPOINT" --size-only --only-show-errors \
      --exclude 'channels/*' --exclude 'releases/*' --exclude 'setup/channels/*' --cache-control "$immutable"
    aws s3 sync "$objects/releases/" "s3://$R2_BUCKET/$prefix/releases/" --endpoint-url "$R2_ENDPOINT" --size-only \
      --only-show-errors --cache-control "$immutable"
    echo "uploaded the objects below $objects"
    ;;
  pointers)
    objects=$1
    channel=$2
    set --
    for pointer in "$objects/channels/$channel"/*.json; do
      if [ -e "$pointer" ]; then set -- "$@" "channels/$channel/${pointer##*/}"; fi
    done
    put_pointers "$objects" "$@"
    ;;
  manifests)
    objects=$1
    version=$2
    shift 2
    for platform in "$@"; do
      key="releases/$version/$platform/manifest.json"
      mkdir -p "$objects/releases/$version/$platform"
      for name in "$key" "$key.sig"; do
        s3 get-object --key "$prefix/$name" "$objects/$name" > /dev/null
      done
    done
    echo "downloaded the $version manifests"
    ;;
  records)
    artifacts=$1
    urls=''
    # Reuse records change in place like pointers and are written only once the version they name is live;
    # a release that rebuilt nothing has none. Each record names a published version and nothing else
    # (release-reuse.mjs), so a stale cached record only costs a rebuild.
    # 复用记录像指针一样原地更新，只在其指明的版本上线后写入；什么都未重新构建的发版没有记录。每条记录只指明一个
    # 已发布版本（release-reuse.mjs），因此缓存中的旧记录只会多一次重新构建。
    for record in "$artifacts"/release-*/records/fingerprints/*/*.json; do
      [ -e "$record" ] || continue
      name=${record#*/records/}
      s3 put-object --key "$prefix/$name" --body "$record" --content-type application/json \
        --cache-control 'public, max-age=60' > /dev/null
      urls="$urls $base/$name"
      echo "recorded $name"
    done
    # shellcheck disable=SC2086 # urls is a space-separated list of URLs without spaces.
    [ -z "$urls" ] || purge $urls
    ;;
  setup-unpublished)
    version=$1
    # Every setup version publishes install.sh; its files are immutable like a product release's.
    # 每个安装器版本都发布 install.sh；其文件与产品版本一样不可变。
    key="$prefix/setup/$version/install.sh"
    if s3 head-object --key "$key" > /dev/null 2>&1; then
      echo "setup $version is already published ($key)" >&2
      exit 1
    fi
    echo "setup $version is not published yet"
    ;;
  setup-upload)
    objects=$1
    version=$2
    aws s3 sync "$objects/setup/$version/" "s3://$R2_BUCKET/$prefix/setup/$version/" --endpoint-url "$R2_ENDPOINT" \
      --only-show-errors --cache-control 'public, max-age=31536000, immutable'
    echo "uploaded setup $version"
    ;;
  setup-pointer)
    put_pointers "$1" "setup/channels/$2.json"
    ;;
  setup-pointer-get)
    objects=$1
    name="setup/channels/$2.json"
    mkdir -p "$objects/setup/channels"
    for key in "$name" "$name.sig"; do
      s3 get-object --key "$prefix/$key" "$objects/$key" > /dev/null
    done
    echo "downloaded $name"
    ;;
  setup-scripts)
    # The pointer below <objects> was verified by `vulcan-release-tool setup-promote` just before; each script
    # must match the digest it lists before it replaces the top-level copy that `curl | sh` users fetch.
    # <objects> 之下的指针刚由 `vulcan-release-tool setup-promote` 校验过；每个脚本必须与其中列出的摘要一致，
    # 才能替换 `curl | sh` 用户获取的顶层副本。
    objects=$1
    pointer="$objects/setup/channels/$2.json"
    version=$(jq -er .version "$pointer")
    urls=''
    for name in $(jq -r '.scripts[].file' "$pointer"); do
      case "$name" in install.sh | install.ps1) ;; *) echo "unexpected script $name" >&2; exit 1 ;; esac
      file="$objects/setup/$version/$name"
      mkdir -p "${file%/*}"
      s3 get-object --key "$prefix/setup/$version/$name" "$file" > /dev/null
      expected=$(jq -er --arg name "$name" '.scripts[] | select(.file == $name) | .sha256' "$pointer")
      actual=$(sha256sum "$file" | cut -d ' ' -f 1)
      [ "$actual" = "$expected" ] || { echo "setup/$version/$name does not match the pointer" >&2; exit 1; }
      s3 put-object --key "$prefix/$name" --body "$file" --content-type text/plain \
        --cache-control 'public, max-age=60' > /dev/null
      urls="$urls $base/$name"
    done
    [ -n "$urls" ] || { echo "the pointer lists no scripts" >&2; exit 1; }
    # shellcheck disable=SC2086 # urls is a space-separated list of URLs without spaces.
    purge $urls
    for name in $(jq -r '.scripts[].file' "$pointer"); do
      seen=''
      for attempt in 1 2 3 4 5 6 7 8 9 10; do
        if curl -fsS "$base/$name" | cmp -s - "$objects/setup/$version/$name"; then seen=yes; break; fi
        sleep 2
      done
      [ "$seen" = yes ] || { echo "the CDN does not serve the new top-level $name" >&2; exit 1; }
      echo "top-level $name is setup $version"
    done
    ;;
  *)
    echo "unknown command: $command" >&2
    exit 2
    ;;
esac
