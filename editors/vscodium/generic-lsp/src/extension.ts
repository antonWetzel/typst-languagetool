import { isDeepStrictEqual } from 'util';
import * as vscode from 'vscode';

import {
	ConfigurationParams,
	ConfigurationRequest,
	DidChangeConfigurationNotification,
	ExitNotification,
	LanguageClient,
	LanguageClientOptions,
	ServerOptions,
	TransportKind
} from 'vscode-languageclient/node';

let clients: {
	lsp: string,
	language: string,
	options: any,
	client: LanguageClient,
}[] = [];

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

	let new_clients = [];
	for (let entry of config) {
		let old_index = clients.findIndex((old) => { return old.language === entry.language && old.lsp === entry.lsp; });
		if (old_index >= 0) {
			let old = clients.splice(old_index, 1)[0];
			if (!isDeepStrictEqual(old.options, entry.options)) {
				console.log("Update Settings for", entry.lsp, entry.language);
				old.options = entry.options;
				old.client.sendNotification(DidChangeConfigurationNotification.type, { settings: entry.options });
			}
			new_clients.push(old);
		} else {
			let client = create_lsp(entry.lsp, entry.language, entry.options);
			new_clients.push({
				client: client,
				language: entry.language,
				lsp: entry.lsp,
				options: entry.options,
			});
		}
	}
	for (let client in clients) {
		await clients[client].client.stop();
	}
	clients = new_clients;
}

function create_lsp(command: string, language: string, options: any): LanguageClient {
	console.log("New LSP for", command, language);
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
	client.onNotification(ExitNotification.type, () => { console.log("exit"); });
	client.onRequest(ConfigurationRequest.type, (params: ConfigurationParams) => {
		console.log("config request ", params);
		return null;
	});
	client.start();

	return client;
}

export async function deactivate(): Promise<void> {
	for (let client in clients) {
		await clients[client].client.stop();
	}
}
