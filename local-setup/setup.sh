#!/bin/bash

set -e

GREEN="\033[1;32m"
CYAN="\033[1;36m"
RED="\033[1;31m"
YELLOW="\033[1;33m"
RESET="\033[0m"

# Print usage information
usage() {
    echo "Usage: $0 [--default]"
    echo
    echo "Options:"
    echo "  --default    Run in non-interactive mode and install all components"
    echo "  --help       Display this help message"
    echo
    exit 1
}

# Parse command line arguments
INTERACTIVE=true
while [[ $# -gt 0 ]]; do
    case $1 in
        --default)
            INTERACTIVE=false
            shift
            ;;
        --help)
            usage
            ;;
        *)
            echo -e "${RED}Error: Unknown option: $1${RESET}"
            usage
            ;;
    esac
done

prompt() {
    if [ "$INTERACTIVE" = false ]; then
        return 0
    fi
    echo -e "${CYAN}$1 [y/N]: ${RESET}"
    read -r response
    [[ "$response" =~ ^[Yy]$ ]]
}

# Function to install updates
update_system() {
    echo -e "${GREEN}Updating the system...${RESET}"
    apt-get update && apt-get upgrade -y && apt-get autoremove -y
}

# Function to set up UFW
setup_firewall() {
    echo -e "${GREEN}Setting up UFW (firewall)...${RESET}"
    if ! command -v ufw &> /dev/null; then
        apt-get install -y ufw
    fi
    ufw default deny incoming
    ufw default allow outgoing
    ufw allow 22/tcp   # SSH
    ufw allow 80/tcp   # HTTP
    ufw allow 443/tcp  # HTTPS
    ufw enable || echo "UFW is already enabled."
}

# Function to configure SSH security
setup_ssh() {
    echo -e "${GREEN}Securing SSH...${RESET}"
    sed -i '/^PermitRootLogin/s/yes/no/' /etc/ssh/ssh_config
    sed -i '/^#PasswordAuthentication/s/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/ssh_config
    sed -i '/^#MaxAuthTries/s/#MaxAuthTries.*/MaxAuthTries 3/' /etc/ssh/ssh_config
    sed -i '/^#ClientAliveInterval/s/#ClientAliveInterval.*/ClientAliveInterval 300/' /etc/ssh/ssh_config
    echo -e "${GREEN}SSH hardened with key-only auth and additional security measures.${RESET}"
    systemctl restart ssh
}

# Function to install and configure Docker
setup_docker() {
    echo -e "${GREEN}Installing Docker and Docker Compose...${RESET}"
    if ! command -v docker &> /dev/null; then
        apt-get install -y apt-transport-https ca-certificates curl \
            software-properties-common gnupg

        curl -fsSL https://download.docker.com/linux/ubuntu/gpg | \
            gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg

        echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] \
            https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable" | \
            tee /etc/apt/sources.list.d/docker.list > /dev/null

        apt-get update
        apt-get install -y docker-ce docker-ce-cli containerd.io \
            docker-compose-plugin

        groupadd -f docker
        usermod -aG docker $SUDO_USER

        systemctl enable docker
        systemctl start docker
    else
        echo -e "${YELLOW}Docker is already installed.${RESET}"
        # Ensure permissions even if Docker is already installed
        groupadd -f docker
        usermod -aG docker $SUDO_USER
    fi
}

# Function to install fail2ban with Docker support
install_fail2ban() {
    echo -e "${GREEN}Installing and configuring Fail2Ban...${RESET}"
    if ! command -v fail2ban-client &> /dev/null; then
        apt-get install -y fail2ban
        
        cat <<EOF > /etc/fail2ban/jail.local
[DEFAULT]
bantime = 3600
findtime = 600
maxretry = 5

[sshd]
enabled = true

[traefik-auth]
enabled = true
filter = traefik-auth
logpath = /var/log/traefik/access.log
maxretry = 3
bantime = 3600
findtime = 600
EOF

        cat <<EOF > /etc/fail2ban/filter.d/traefik-auth.conf
[Definition]
failregex = ^.*Unauthorized request.*\"<HOST>.*$
ignoreregex =
EOF

        systemctl enable fail2ban
        systemctl restart fail2ban
    else
        echo -e "${YELLOW}Fail2Ban is already installed.${RESET}"
    fi
}

# Function to set up system auditing
setup_auditd() {
    echo -e "${GREEN}Installing and configuring auditd...${RESET}"
    if ! command -v auditctl &> /dev/null; then
        apt-get install -y auditd

        cat <<EOF >> /etc/audit/rules.d/docker.rules
-w /usr/bin/docker -p wa
-w /var/lib/docker -p wa
-w /etc/docker -p wa
-w /usr/lib/systemd/system/docker.service -p wa
-w /etc/default/docker -p wa
-w /etc/docker/daemon.json -p wa
-w /usr/bin/dockerd -p wa
EOF

        systemctl enable auditd
        systemctl restart auditd
    else
        echo -e "${YELLOW}Auditd is already installed.${RESET}"
    fi
}

# Function to set up sysctl configurations
setup_sysctl() {
    echo -e "${GREEN}Configuring system kernel parameters...${RESET}"
    cat <<EOF > /etc/sysctl.d/99-security.conf
# Network security
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv4.conf.all.secure_redirects = 0
net.ipv4.conf.default.secure_redirects = 0
net.ipv4.conf.all.log_martians = 1
net.ipv4.conf.default.log_martians = 1
net.ipv4.icmp_echo_ignore_broadcasts = 1
net.ipv4.icmp_ignore_bogus_error_responses = 1
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1
net.ipv4.tcp_syncookies = 1

# Docker specific
net.ipv4.ip_forward = 1
net.bridge.bridge-nf-call-iptables = 1
net.bridge.bridge-nf-call-ip6tables = 1

# System security
kernel.sysrq = 0
kernel.core_uses_pid = 1
kernel.kptr_restrict = 2
kernel.panic = 60
kernel.panic_on_oops = 60
kernel.keys.root_maxkeys = 1000000
kernel.keys.root_maxbytes = 25000000
EOF

    sysctl -p /etc/sysctl.d/99-security.conf
}

# Main function
main() {
    echo -e "${CYAN}Welcome to the Enhanced Ubuntu Server Hardening Script (Docker + Traefik Edition)!${RESET}"
    if [ "$INTERACTIVE" = false ]; then
        echo -e "${YELLOW}Running in non-interactive mode. All components will be installed.${RESET}"
    fi
    
    if [ "$EUID" -ne 0 ]; then 
        echo -e "${RED}Please run as root${RESET}"
        exit 1
    fi

    update_system

    if prompt "Set up UFW (firewall)?"; then
        setup_firewall
    fi

    if prompt "Secure SSH (disable root login and password auth)?"; then
        setup_ssh
    fi

    if prompt "Install and configure Docker with secure defaults?"; then
        setup_docker
    fi

    if prompt "Install and configure Fail2Ban with Docker support?"; then
        install_fail2ban
    fi

    if prompt "Configure system auditing (auditd) with Docker rules?"; then
        setup_auditd
    fi

    if prompt "Configure secure sysctl parameters?"; then
        setup_sysctl
    fi

    echo -e "${GREEN}Server hardening complete!${RESET}"
    echo -e "${YELLOW}Please reboot your system to apply all changes.${RESET}"
}

main
