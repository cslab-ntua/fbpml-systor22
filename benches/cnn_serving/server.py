from concurrent import futures
import datetime as dt
import time

from google.protobuf.duration_pb2 import Duration
import grpc

import tensorflow as tf
from tensorflow.python.keras.preprocessing import image
from tensorflow.python.keras.applications.resnet50 import (
    preprocess_input,
    decode_predictions,
)
import numpy as np

from squeezenet import SqueezeNet

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


session_conf = tf.ConfigProto(
    intra_op_parallelism_threads=1, inter_op_parallelism_threads=1
)
sess = tf.Session(config=session_conf)


img1 = image.load_img("/bench/img1.jpeg", target_size=(227, 227))
model1 = SqueezeNet(weights="imagenet")
model1._make_predict_function()
print("Model1 is ready")

img2 = image.load_img("/bench/img2.jpeg", target_size=(227, 227))
model2 = SqueezeNet(weights="imagenet")
model2._make_predict_function()
print("Model2 is ready")

imgs = [img1, img2]
models = [model1, model2]


class CNNServing(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        idx = request.arg if request.arg and 0 <= request.arg <= 1 else 0

        img = imgs[idx]
        model = models[idx]

        x = image.img_to_array(img)
        x = np.expand_dims(x, axis=0)
        x = preprocess_input(x)
        preds = model.predict(x)

        # _ = decode_predictions(preds)  # requires network & local storage

        # joblib.dump(model, '/var/local/dir/lr_model.pk')

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(CNNServing(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
