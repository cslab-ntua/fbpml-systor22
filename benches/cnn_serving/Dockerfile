# syntax=docker/dockerfile:1

FROM tatsushid/alpine-py3-tensorflow-jupyter:1.4.0

COPY *.py /bench/
COPY *.h5 /bench/
COPY *.jpeg /bench/

RUN apk add --no-cache --update python3 python3-dev g++ musl-dev make \
		linux-headers \
	&& pip3 install --no-cache-dir grpcio-tools==1.40.0 \
    && echo "http://dl-cdn.alpinelinux.org/alpine/edge/community" >> /etc/apk/repositories \
    && echo "http://dl-cdn.alpinelinux.org/alpine/edge/testing" >> /etc/apk/repositories \
    && apk update \
    && wget http://dl-cdn.alpinelinux.org/alpine/v3.9/main/x86_64/libgfortran-8.3.0-r0.apk \
    && apk add --no-cache libgfortran-8.3.0-r0.apk \
    && rm -v libgfortran-8.3.0-r0.apk \
    && apk add --allow-untrusted --repository http://dl-3.alpinelinux.org/alpine/edge/testing hdf5 hdf5-dev \
    && apk add --no-cache --update build-base py3-numpy py-numpy-dev \
    && pip3 install --upgrade pip \
    && ln -sf /usr/bin/pip3 /usr/bin/pip \
    && ln -sf /usr/bin/pip3 /usr/local/bin/pip \
    && ln -sf /usr/bin/python3 /usr/local/bin/python \
    && ln -sf /usr/bin/python3 /usr/local/bin/python3 \
    && ln -s /usr/include/locale.h /usr/include/xlocale.h \
    && pip3 install --no-cache-dir pkgconfig Cython>=0.23 \
    && pip3 uninstall -y enum34 \
    && pip3 install --no-cache-dir h5py
