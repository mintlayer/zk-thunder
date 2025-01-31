# Rust
curl  --proto  '=https'  --tlsv1.2  -sSf  https://sh.rustup.rs | sh
source ~/.cargo/env
# NVM
curl  -o-  https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.5/install.sh | bash
source ~/.nvm/nvm.sh
# All necessary stuff
sudo  apt-get  update
sudo  apt-get  install  build-essential  pkg-config  cmake  clang  lldb  lld  libssl-dev  postgresql  apt-transport-https  ca-certificates  curl  software-properties-common
# Install docker
curl  -fsSL  https://download.docker.com/linux/ubuntu/gpg | sudo  apt-key  add  -
sudo  add-apt-repository  "deb [arch=amd64] https://download.docker.com/linux/ubuntu focal stable"
sudo  apt  install  docker-ce
sudo  systemctl  stop  docker
sudo  usermod  -aG  docker ${USER}

# Stop default postgres (as we'll use the docker one)
sudo  systemctl  stop  postgresql
sudo  systemctl  disable  postgresql
# Start docker.
sudo  systemctl  start  docker
# You might need to re-connect (due to usermod change).
# Node & yarn
nvm  install  20
# Important: there will be a note in the output to load
# new paths in your local session, either run it or reload the terminal.
npm  install  -g  yarn
yarn  set  version  1.22.19
# For running unit tests
cargo  install  cargo-nextest
# SQL tools
cargo  install  sqlx-cli  --version  0.8.0

echo "IMPORTANT: please log out and then log back in order for docker user group changes to take effect"