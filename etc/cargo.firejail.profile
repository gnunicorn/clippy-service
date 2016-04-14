################################
# Cargo installer profile
################################

# default security profile
include /etc/firejail/disable-common.inc
read-only ${HOME}/app
read-only /vagrant/
private-dev
caps.drop all
seccomp

# limit the process resources
rlimit-nproc 100
rlimit-nofile 500
rlimit-sigpending 5

# network resources
hostname "minion.clippy.bashy.io"

dns 8.8.4.4
dns 8.8.8.8

netfilter /etc/firejail/cargo.net
protocol unix,inet,inet6
