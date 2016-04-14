#!/bin/bash
set -e

SOURCEPWD=`pwd`
TMPDIR=`mktemp -d`

mkdir -p "$TMPDIR"
cd "$TMPDIR"
# download firejail
curl -sLO http://downloads.sourceforge.net/project/firejail/firejail/firejail_0.9.40-rc1_1_amd64.deb

# check the checksum
echo "9ce9d6e72f65bafd51a2240da7954e657413b5ff  firejail_0.9.40-rc1_1_amd64.deb" | sha1sum -c --status

sudo dpkg -i firejail_0.9.40-rc1_1_amd64.deb

cd "$SOURCEPWD"
rm -rf "$TMPDIR"

# setting up our profile
sudo cp "$SOURCEPWD/etc/cargo.firejail.profile" /etc/firejail/cargo.profile
sudo cp "$SOURCEPWD/etc/cargo.netfilter.profile" /etc/firejail/cargo.net
