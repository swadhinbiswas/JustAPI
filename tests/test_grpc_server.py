from justapi.app import JustAPIApp
from tests.proto.gen import helloworld_pb2, helloworld_pb2_grpc

class GreeterServicer(helloworld_pb2_grpc.GreeterServicer):
    def SayHello(self, request, context):
        print(f"Received request in Python: {request.name}")
        return helloworld_pb2.HelloReply(message=f"Hello from Python, {request.name}!")

def test_grpc_server_setup():
    app = JustAPIApp()
    app.set_grpc_addr("127.0.0.1:50051")
    app.add_grpc_service(GreeterServicer(), helloworld_pb2_grpc.add_GreeterServicer_to_server)
    print("Starting JustAPI with gRPC on :50051...")
    # app.run("127.0.0.1:8000") # Do not run in pytest
