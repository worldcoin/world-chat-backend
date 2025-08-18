import time, jwt, os

TEAM_ID = os.getenv("TEAM_ID")
KEY_ID = os.getenv("KEY_ID")
P8_FILE = os.getenv("P8_FILE")

now = int(time.time())
with open(P8_FILE, "rb") as f:
    key = f.read()

token = jwt.encode(
    {"iss": TEAM_ID, "iat": now},
    key,
    algorithm="ES256",
    headers={"kid": KEY_ID}
)
print(token)
