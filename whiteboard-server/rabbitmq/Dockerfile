#
# apk
FROM rabbitmq:3.8.8-management-alpine
# apt
# FROM rabbitmq:3.8.8-management
#
MAINTAINER pierrefenoll@gmail.com
ENV LANG C.UTF-8

#
# apk
# ENV RABBITMQ_PLUGINS_DIR /opt/rabbitmq/plugins
# apt
# ENV RABBITMQ_PLUGINS_DIR /usr/lib/rabbitmq/lib/rabbitmq_server-${RABBITMQ_VERSION}/plugins
# ENV RABBITMQ_PLUGINS_DIR /opt/rabbitmq/plugins
#

# ENV PROMX https://github.com/deadtrickster/prometheus_rabbitmq_exporter/releases/download/v3.7.9.1
# ADD $PROMX/accept-0.3.5.ez \
#     $RABBITMQ_PLUGINS_DIR/
# ADD $PROMX/prometheus-4.3.0.ez \
#     $RABBITMQ_PLUGINS_DIR/
# ADD $PROMX/prometheus_cowboy-0.1.7.ez \
#     $RABBITMQ_PLUGINS_DIR/
# ADD $PROMX/prometheus_httpd-2.1.10.ez \
#     $RABBITMQ_PLUGINS_DIR/
# ADD $PROMX/prometheus_process_collector-1.4.3.ez \
#     $RABBITMQ_PLUGINS_DIR/
# ADD $PROMX/prometheus_rabbitmq_exporter-3.7.9.1.ez \
#     $RABBITMQ_PLUGINS_DIR/

ENV RABBITMQ_CONFIG_FILE /etc/rabbitmq/rabbitmq.conf
COPY rabbitmq.conf $RABBITMQ_CONFIG_FILE
# ENV RABBITMQ_ADVANCED_CONFIG_FILE /etc/rabbitmq/rabbitmq.config
# COPY rabbitmq.config $RABBITMQ_ADVANCED_CONFIG_FILE

# RUN set -x \
#  && apk update && apk upgrade && apk add libc-dev \
#  && apt update && apt upgrade -y \
 # && ls -lha $RABBITMQ_PLUGINS_DIR \
 # && chmod a+r $RABBITMQ_PLUGINS_DIR/accept*.ez $RABBITMQ_PLUGINS_DIR/prometheus*.ez \
 # && rabbitmq-plugins enable --offline prometheus_process_collector \
 # && rabbitmq-plugins enable --offline prometheus_rabbitmq_exporter
