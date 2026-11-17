obj-m += virtio_msg.o virtio_msg_vsock.o wsx_virtio_msg_pci.o
virtio_msg_vsock-y := vsock.o pci-shm.o
wsx_virtio_msg_pci-y := owner.o parent.o
ccflags-y += -I$(src) -I$(src)/include
