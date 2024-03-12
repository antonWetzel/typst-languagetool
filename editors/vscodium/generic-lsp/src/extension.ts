import * as vscode from 'vscode';

import {
	LanguageClient,
	LanguageClientOptions,
	ServerOptions,
	TransportKind
} from 'vscode-languageclient/node';

let clients: { [id: string]: LanguageClient } = {};

type Config = {
	lsp: string,
	language: string,
	options: any,
};

export async function activate(context: vscode.ExtensionContext) {
	vscode.workspace.onDidChangeConfiguration(async (e) => {
		if (!e.affectsConfiguration('generic-lsp')) {
			return;
		}
		await config_change();
	});
	await config_change();
}

async function config_change() {
	const config = vscode.workspace.getConfiguration("generic-lsp").get("configuration") as Config[];

	for (let client in clients) {
		await clients[client].stop();
	}
	clients = {};

	for (let entry of config) {
		clients[entry.language] = create_lsp(entry.lsp, entry.language, entry.options);
	}
}

function create_lsp(command: string, language: string, options: any): LanguageClient {
	const serverOptions: ServerOptions = {
		run: { command: command, transport: TransportKind.stdio },
		debug: {
			command: command,
			transport: TransportKind.stdio,
		}
	};

	console.log(options);

	const clientOptions: LanguageClientOptions = {
		initializationOptions: options,
		documentSelector: [{ scheme: 'file', language: language }],
	};

	let client = new LanguageClient(
		'generic-lsp-' + language,
		'Generig LSP for ' + language,
		serverOptions,
		clientOptions
	);
	client.start();
	return client;
}

export async function deactivate(): Promise<void> {
	for (let client in clients) {
		await clients[client].stop();
	}
}
