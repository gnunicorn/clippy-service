*filter
:INPUT DROP [0:0]
:FORWARD DROP [0:0]
:OUTPUT ACCEPT [0:0]

###################################################################
# Client filter rejecting local network traffic, with the exception of DNS traffic
#
# Usage:
#     firejail --net=eth0 --netfilter=/etc/firejail/nolocal.net firefox
#
###################################################################

# disallow all
-P INPUT   DROP
-P FORWARD DROP
-P OUTPUT DROP

# accept ICMP, ping and stuff
-A OUTPUT -p icmp -m state --state NEW,ESTABLISHED,RELATED -j ACCEPT
-A INPUT  -p icmp -m state --state ESTABLISHED,RELATED     -j ACCEPT



# Allow DNS lookups via our configured Google DNS servers
-A OUTPUT -p udp -d 8.8.4.4 --dport 53 -m state --state NEW,ESTABLISHED -j ACCEPT
-A INPUT  -p udp -s 8.8.4.4 --sport 53 -m state --state ESTABLISHED     -j ACCEPT
-A OUTPUT -p tcp -d 8.8.4.4 --dport 53 -m state --state NEW,ESTABLISHED -j ACCEPT
-A INPUT -p tcp -s 8.8.4.4 --sport 53 -m state --state ESTABLISHED -j ACCEPT

-A OUTPUT -p udp -d 8.8.8.8 --dport 53 -m state --state NEW,ESTABLISHED -j ACCEPT
-A INPUT  -p udp -s 8.8.8.8 --sport 53 -m state --state ESTABLISHED     -j ACCEPT
-A OUTPUT -p tcp -d 8.8.8.8 --dport 53 -m state --state NEW,ESTABLISHED -j ACCEPT
-A INPUT -p tcp -s 8.8.8.8 --sport 53 -m state --state ESTABLISHED -j ACCEPT



# we do accept github.com
# ssh
-A OUTPUT -p tcp -d 192.30.252.0/22 --dport 22 -m state --state NEW,ESTABLISHED -j ACCEPT
-A INPUT -p tcp -s 192.30.252.0/22 --sport 22 -m state --state NEW,ESTABLISHED -j ACCEPT

# and HTTPS
-A OUTPUT -p tcp -d 192.30.252.0/22 --dport 443 -m state --state NEW,ESTABLISHED -j ACCEPT
-A INPUT -p tcp -s 192.30.252.0/22 --sport 443 -m state --state NEW,ESTABLISHED -j ACCEPT

COMMIT
