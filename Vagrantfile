# -*- mode: ruby -*-
# vi: set ft=ruby :

# A simple vagrant file, using debian/jessie64
# and latest install rust nightly + toolchain

MESSAGE = <<-MESSAGE
WELCOME to
    _____  _    _  _____ _______
    |  __ \| |  | |/ ____|__   __|
    | |__) | |  | | (___    | |
    |  _  /| |  | |\___ \   | |
    | | \ \| |__| |____) |  | |
    |_|  \_\\____/|_____/   |_|

You can now log into your development enviroment via

    vagrant ssh


Have fun!

MESSAGE

# The list of packages we need to have installed globally
INSTALL = <<-INSTALL
sudo apt-get update
sudo apt-get upgrade -y
sudo apt-get install -y build-essential g++ pgp python perl make curl git libssl-dev redis-server

INSTALL


Vagrant.configure(2) do |config|
  config.vm.box = "debian/jessie64"
  config.vm.post_up_message = MESSAGE

  config.vm.network "private_network", ip: "10.1.1.10"
  config.vm.synced_folder ".", "/vagrant",  type: 'nfs'

  config.vm.network "forwarded_port", guest: 8080, host: 9099
  config.vm.provision "shell", inline: INSTALL
  # use rustup.sh to install nightly.
  config.vm.provision "shell", inline: "curl -sO https://static.rust-lang.org/rustup.sh && sh rustup.sh --yes --channel=nightly"

  config.vm.provision "shell", inline: <<firejail
  cd /vagrant
  sh etc/install_firejail.sh
FIREJAIL
  config.vm.provider "virtualbox" do |v|

    host = RbConfig::CONFIG['host_os']


    if host =~ /darwin/
      # sysctl returns Bytes and we need to convert to MB
      mem = `sysctl -n hw.memsize`.to_i / 1024
    elsif host =~ /linux/
      # meminfo shows KB and we need to convert to MB
      mem = `grep 'MemTotal' /proc/meminfo | sed -e 's/MemTotal://' -e 's/ kB//'`.to_i
    elsif host =~ /mswin|mingw|cygwin/
      # Windows code via https://github.com/rdsubhas/vagrant-faster
      mem = `wmic computersystem Get TotalPhysicalMemory`.split[1].to_i / 1024
    end

    # Give VM 1/4 system memory
    # as discussed here: https://stefanwrobel.com/how-to-make-vagrant-performance-not-suck
    v.memory = mem / 1024 / 4
  end
end
