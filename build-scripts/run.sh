#!/bin/sh
while true; do
    /usr/app/fossil_headers_db fix
    sleep ${INTERVAL:-300}
done
