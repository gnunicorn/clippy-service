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

# The list of packages we want to install globally
INSTALL = <<-INSTALL
sudo apt-get update
sudo apt-get upgrade -y
sudo apt-get install -y build-essential g++ python perl make curl git

INSTALL

# Provising on the system and user level
NIGHTLY = <<-SETUP

mkdir -p ~/dev
cd ~/dev
if [ ! -d ~/dev/rust ]; then
  git clone --recursive https://github.com/rust-lang/rust.git ~/dev/rust
fi

if [ ! -d ~/dev/cargo ]; then
  git clone --recursive https://github.com/rust-lang/cargo.git ~/dev/cargo
fi

cd ~/dev/rust
git pull
./configure
make
sudo make install


cd ~/dev/cargo
git pull
./configure
make
sudo make install
SETUP

Vagrant.configure(2) do |config|
  config.vm.box = "debian/jessie64"
  config.vm.post_up_message = MESSAGE

  config.vm.network "private_network", ip: "10.1.1.10"
  config.vm.synced_folder ".", "/vagrant",  type: 'nfs', mount_options: ['rw', 'vers=3', 'tcp', 'fsc' ,'actimeo=1']

  config.vm.provision "shell", inline: INSTALL
  config.vm.provision "shell", inline: NIGHTLY

  config.vm.provider "virtualbox" do |v|
    v.memory = 2048
    v.cpus = 2
  end
end
