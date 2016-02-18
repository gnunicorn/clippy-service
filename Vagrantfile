# -*- mode: ruby -*-
# vi: set ft=ruby :

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
sudo apt-get install -y build-essential g++ pgp python perl make curl git libssl-dev

INSTALL


Vagrant.configure(2) do |config|
  config.vm.box = "debian/jessie64"
  config.vm.post_up_message = MESSAGE

  config.vm.network "private_network", ip: "10.1.1.10"
  config.vm.synced_folder ".", "/vagrant",  type: 'nfs', mount_options: ['rw', 'vers=3', 'tcp', 'fsc' ,'actimeo=1']

  config.vm.network "forwarded_port", guest: 8080, host: 9099
  config.vm.provision "shell", inline: INSTALL
  # use rustup.sh to install nightly.
  config.vm.provision "shell", inline: "curl -sO https://static.rust-lang.org/rustup.sh && sh rustup.sh --yes --channel=nightly"

  config.vm.provider "virtualbox" do |v|
    v.memory = 2048
    v.cpus = 2
  end
end
