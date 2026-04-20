# fab-ai
Flesh and blood TCG AI and simulation environment

# Generate Certificate
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=<your_ip>" -addext "subjectAltName=IP:<your_ip>,IP:127.0.0.1,DNS:localhost"

# Run with certificate
python3 web_viewer.py --ssl-cert cert.pem --ssl-key key.pem --port=8080
