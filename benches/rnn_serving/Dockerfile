# syntax=docker/dockerfile:1

FROM petronetto/pytorch-alpine

COPY *.py /bench/
COPY rnn* /bench/

RUN apk update && apk add --no-cache --update g++ linux-headers \
    && ln -s /usr/bin/python3 /usr/local/bin/python3 \
    && pip3 install --no-cache-dir grpcio-tools==1.43.0
