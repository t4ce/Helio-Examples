.PHONY: serve verify smoke config-test

serve:
	python3 serve.py --open

verify:
	./verify.sh

smoke:
	python3 tests/smoke_test.py

config-test:
	python3 tests/config_io_test.py
