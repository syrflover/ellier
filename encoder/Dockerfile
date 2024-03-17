FROM python:3.12-alpine3.19 as base

RUN apk update && apk add --no-cache ffmpeg libxslt-dev libxml2-dev

# 

FROM base as builder

ARG VERSION=6.7.0

RUN apk add --no-cache --virtual .build-deps gcc musl-dev
RUN pip wheel --wheel-dir=/root/wheels streamlink==$VERSION

#

FROM base

COPY --from=builder /root/wheels /root/wheels
RUN pip install --no-index --find-links=/root/wheels streamlink

RUN apk update

RUN apk add ffmpeg~=6.1.1
RUN apk add mkvtoolnix~=81.0

ARG BINARY_FILE

COPY $BINARY_FILE /erish-encoder

ENTRYPOINT [ "/erish-encoder" ]
