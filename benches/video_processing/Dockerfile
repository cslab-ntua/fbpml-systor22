# syntax=docker/dockerfile:1

FROM denismakogon/opencv3-slim:edge

COPY *.py /bench/

RUN apt -y update \
    && apt -y upgrade \
    && apt -y install --no-install-recommends libgl1-mesa-glx libglib2.0-0 \
    && pip3 install --upgrade pip \
    && pip3 install --no-cache --no-cache-dir opencv-python minio \
        grpcio-tools==1.43.0
