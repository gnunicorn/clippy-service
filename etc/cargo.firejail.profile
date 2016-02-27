################################
# Cargo installer profile
################################

# default security profile
include /etc/firejail/disable-mgmt.inc
include /etc/firejail/disable-secret.inc
include /etc/firejail/disable-common.inc
read-only ${HOME}/
read-only /vagrant/
caps.drop all
seccomp
noroot

# limit the process resources
rlimit-nproc 100
rlimit-nofile 500
rlimit-sigpending 5

# network resources
hostname "minion.clippy.bashy.io"
netfilter /etc/firejail/cargo.net
protocol unix,inet,inet6
