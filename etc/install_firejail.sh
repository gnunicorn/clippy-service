#!/bin/bash
set -e

# download firejail
wget https://sourceforge.net/projects/firejail/files/firejail/firejail_0.9.38_1_amd64.deb https://sourceforge.net/projects/firejail/files/firejail/firejail-0.9.38.asc

# check the gpg signature
gpg --import etc/firejail-developers.asc
gpg --no-default-keyring --verify firejail-0.9.38.asc

# check the checksum
grep firejail_0.9.38_1_amd64.deb firejail-0.9.38.asc | sha256sum -c --status

sudo dpkg -i firejail_0.9.38_1_amd64.deb

# setting up our profile
sudo cp etc/cargo.firejail.profile /etc/firejail/cargo.profile
sudo cp etc/cargo.netfilter.profile /etc/firejail/cargo.net
