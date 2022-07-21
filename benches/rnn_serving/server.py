from concurrent import futures
import datetime as dt
import pickle
import string
import time

import torch

import rnn

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


torch.set_num_threads(1)


language1 = "Scottish"
language2 = "Russian"
start_letters1 = "ABCDEFGHIJKLMNOP"
start_letters2 = "QRSTUVWXYZABCDEF"

LANGUAGES = [language1, language2]
START_LETTERS = [start_letters1, start_letters2]


with open("/bench/rnn_params.pkl", "rb") as pkl:
    params = pickle.load(pkl)


all_categories = [
    "French",
    "Czech",
    "Dutch",
    "Polish",
    "Scottish",
    "Chinese",
    "English",
    "Italian",
    "Portuguese",
    "Japanese",
    "German",
    "Russian",
    "Korean",
    "Arabic",
    "Greek",
    "Vietnamese",
    "Spanish",
    "Irish",
]
n_categories = len(all_categories)
all_letters = string.ascii_letters + " .,;'-"
n_letters = len(all_letters) + 1

rnn_model = rnn.RNN(
    n_letters,
    128,
    n_letters,
    all_categories,
    n_categories,
    all_letters,
    n_letters,
)
rnn_model.load_state_dict(torch.load("/bench/rnn_model.pth"))
rnn_model.eval()


class RNNServing(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        idx = request.arg if request.arg and 0 <= request.arg <= 1 else 0
        language = LANGUAGES[idx]
        start_letters = START_LETTERS[idx]

        _ = list(rnn_model.samples(language, start_letters))

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(RNNServing(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
