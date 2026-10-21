import type { BaseLocator } from "../tutanota/src/platform-kit/base/BaseLocator.js"
import { createBaseLocator } from "../tutanota/src/platform-kit/base/BaseLocator.js"
import type { LoginListener } from "../tutanota/src/platform-kit/base/facades/LoginFacade.js"
import type { MainInterface } from "../tutanota/src/applications/common/api/worker/workerInterfaces.js"
import type { NativeInterface } from "../tutanota/src/app-kit/native-bridge/common/NativeInterface.js"
import type { BrowserData } from "../tutanota/src/platform-kit/app-env/boot/ClientConstants.js"
import { NamedClientModel } from "@tutao/instance-pipeline"
import { ClientPlatform } from "../tutanota/src/platform-kit/app-env/boot/ClientDetector.js"
import { PresentableKeyVerificationState, SessionType } from "@tutao/app-env"
import { tutanotaModelInfo, tutanotaTypeModels } from "../tutanota/src/entities/tutanota"
import { baseModelInfo, baseTypeModels } from "../tutanota/src/entities/base"
import { createPublicKeyGetIn, PublicKeyService_GET, sysModelInfo, sysTypeModels } from "../tutanota/src/entities/sys"
import { driveModelInfo, driveTypeModels } from "../tutanota/src/entities/drive"
import { storageModelInfo, storageTypeModels } from "../tutanota/src/entities/storage"
import { monitorModelInfo, monitorTypeModels } from "../tutanota/src/entities/monitor"
import { usageModelInfo, usageTypeModels } from "../tutanota/src/entities/usage"
import { accountingModelInfo, accountingTypeModels } from "../tutanota/src/entities/accounting"
import { AppNameEnum } from "@tutao/meta"
import { lazyMemoized, Nullable } from "@tutao/utils"
import { LateInitializedCacheStorageImpl } from "../tutanota/src/app-kit/local-store/CacheStorageProxy.js"
import { EphemeralCacheStorage } from "../tutanota/src/app-kit/local-store/EphemeralCacheStorage.js"
import { CustomCacheHandlerMap } from "../tutanota/src/app-kit/local-store/CustomCacheHandler.js"
import { NoOpLastProcessedEventBatchStorageFacade } from "../tutanota/src/applications/common/api/worker/LastProcessedEventBatchStorageFacade.js"
import { loadWasmFromFileOrNetwork } from "../tutanota/src/platform-kit/utils/WebAssembly.js"
import { type Argon2IDExports, generateKeyFromPassphraseArgon2id, PublicKeyIdentifierType } from "../tutanota/src/platform-kit/crypto"
import { RsaWeb } from "../tutanota/src/app-kit/native-bridge/worker/RsaImplementation.js"
import { TutanotaEntityMigrator } from "../tutanota/src/applications/common/api/worker/TutanotaEntityMigrator.js"
import { DefaultEntityRestCache } from "../tutanota/src/applications/common/api/worker/rest/DefaultEntityRestCache.js"
import { DomainConfigProvider } from "../tutanota/src/applications/common/api/common/DomainConfigProvider.js"
import { IdentityKeyTrustDatabase } from "../tutanota/src/platform-kit/base/base-crypto/persistence/IdentityKeyTrustDatabase"
import { MailFacade } from "../tutanota/src/applications/common/api/worker/facades/lazy/MailFacade"
import { BlobFacade } from "../tutanota/src/applications/common/api/worker/facades/lazy/BlobFacade"
import { NativeFileApp } from "../tutanota/src/app-kit/native-bridge/common/FileApp"
import { DesktopFileFacade } from "../tutanota/src/applications/common/desktop/files/DesktopFileFacade"
import { ExportFacade, NativeCryptoFacade } from "../tutanota/src/native-bridge/generatedIpc/types"
import { EntityClient } from "../tutanota/src/platform-kit/network/EntityClient"
import { initClientModels } from "../tutanota/src/applications/common/api/common/ClientModelInfoInitializer"
import { ConversationType, MailMethod, PartialRecipient, Recipient, RecipientType } from "../tutanota/src/entities/tutanota/Utils"
import {BadRequestError, NotFoundError} from "@tutao/rest-client/error"
import xhr2 from "xhr2"
import http from "node:http"
import fs from "node:fs"
import os from 'node:os';

const HOME = os.homedir();
const SOCKET_PATH = "/tmp/stone-bridge.sock"
const CONFIG_PATH = `${HOME}/.config/stonebridge/config`
const SERVER_HOST = "app.tuta.com"
const SERVER_URL = `https://${SERVER_HOST}`

;(globalThis as any).env = {
	staticUrl: SERVER_URL,
	versionNumber: "360.260922.0",
	dist: false,
	mode: "Prod",
	networkDebugging: false,
	domainConfigs: {
		[SERVER_HOST]: {
			firstPartyDomain: true,
			partneredDomainTransitionUrl: SERVER_URL,
			apiUrl: SERVER_URL,
			paymentUrl: `${SERVER_URL}/braintree.html`,
			webauthnUrl: `${SERVER_URL}/webauthn`,
			legacyWebauthnUrl: `${SERVER_URL}/webauthn`,
			webauthnMobileUrl: `${SERVER_URL}/webauthnmobile`,
			legacyWebauthnMobileUrl: `${SERVER_URL}/webauthnmobile`,
			webauthnRpId: SERVER_HOST,
			u2fAppId: `${SERVER_URL}/u2f-appid.json`,
			giftCardBaseUrl: `${SERVER_URL}/giftcard`,
			referralBaseUrl: `${SERVER_URL}/signup`,
			websiteBaseUrl: "https://tuta.com",
		},
		"{hostname}": {
			firstPartyDomain: false,
			partneredDomainTransitionUrl: "{protocol}//{hostname}",
			apiUrl: "{protocol}//{hostname}",
			paymentUrl: "https://pay.tutanota.com/braintree.html",
			webauthnUrl: "{protocol}//{hostname}/webauthn",
			legacyWebauthnUrl: "{protocol}//{hostname}/webauthn",
			webauthnMobileUrl: "{protocol}//{hostname}/webauthnmobile",
			legacyWebauthnMobileUrl: "{protocol}//{hostname}/webauthnmobile",
			webauthnRpId: "{hostname}",
			u2fAppId: "{protocol}//{hostname}/u2f-appid.json",
			giftCardBaseUrl: "https://app.tuta.com/giftcard",
			referralBaseUrl: "https://app.tuta.com/signup",
			websiteBaseUrl: "https://tuta.com",
		},
	},
}
;(globalThis as any).isBrowser = false
;(globalThis as any).self = globalThis
;(globalThis as any).XMLHttpRequest = xhr2

type MailAddress  = {
	address: string,
	name: string | null
}

type SocketMessage = {
	sender: MailAddress,
	recipients: MailAddress[],
	subject: string
	body: string
}

export async function getBaseLocator(loginAddress: string, passphrase: string): Promise<BaseLocator> {
	// ── argon2 facade (wasm from test/build/) ─────────────────────────────────────

	const argon2Wasm = await loadWasmFromFileOrNetwork<Argon2IDExports>("argon2.wasm", new URL("./", import.meta.url).href)
	const argon2idFacade = {
		async generateKeyFromPassphrase(passphrase: string, salt: Uint8Array<ArrayBuffer>) {
			return generateKeyFromPassphraseArgon2id(argon2Wasm, passphrase, salt)
		},
	}

	// ── apps ──────────────────────────────────────────────────────────────────────

	const apps: Array<NamedClientModel> = [
		{ app: AppNameEnum.Base, clientModel: baseTypeModels, modelInfo: baseModelInfo },
		{ app: "sys", clientModel: sysTypeModels, modelInfo: sysModelInfo },
		{ app: "tutanota", clientModel: tutanotaTypeModels, modelInfo: tutanotaModelInfo },
		{ app: "drive", clientModel: driveTypeModels, modelInfo: driveModelInfo },
		{ app: "storage", clientModel: storageTypeModels, modelInfo: storageModelInfo },
		{ app: "monitor", clientModel: monitorTypeModels, modelInfo: monitorModelInfo },
		{ app: "usage", clientModel: usageTypeModels, modelInfo: usageModelInfo },
		{ app: "accounting", clientModel: accountingTypeModels, modelInfo: accountingModelInfo },
	]

	// ── stubs ─────────────────────────────────────────────────────────────────────

	const noOpLoginListener: LoginListener = {
		async onPartialLoginSuccess() {},
		async onFullLoginSuccess() {},
		async onLoginFailure() {},
		async onSecondFactorChallenge() {},
		onResetSession() {},
	}

	const mainInterface: MainInterface = {
		loginListener: noOpLoginListener,
		wsConnectivityListener: { updateWebSocketState() {} } as any,
		progressTracker: { write() {} } as any,
		eventController: { onEntityEventsReceived() {}, onError() {} } as any,
		operationProgressTracker: { onProgress() {} } as any,
		infoMessageHandler: { onInfoMessage() {} } as any,
		syncTracker: { onSyncStarted() {}, onSyncCompleted() {} } as any,
		uploadProgressListener: { onProgress() {} } as any,
	}

	const worker: NativeInterface & { sendError(e: Error): Promise<void>; getMainInterface(): MainInterface } = {
		invokeNative(requestType: string, args: ReadonlyArray<unknown>): Promise<any> {
			throw new Error("invokeNative not supported: " + requestType)
		},
		sendError(e: Error): Promise<void> {
			console.error("Worker error:", e)
			return Promise.resolve()
		},
		getMainInterface(): MainInterface {
			return mainInterface
		},
	}

	const identityKeyTrustDatabase: IdentityKeyTrustDatabase = {
		async isIdentityKeyTrustDatabaseSupported() {
			return false
		},
		async getManuallyVerifiedEntries() {
			return new Map()
		},
		async trust(mailAddress, key, source) {
			return { publicIdentityKey: key, sourceOfTrust: source }
		},
		async untrust() {},
		async getTrustedEntry() {
			return null
		},
	}

	// ── setup & run ───────────────────────────────────────────────────────────────

	let base: BaseLocator

	const ephemeralStorageProvider = async () =>
		new EphemeralCacheStorage((base as any).instancePipeline.modelMapper, (base as any).typeModelResolver, new CustomCacheHandlerMap())

	const maybeUninitializedStorage = new LateInitializedCacheStorageImpl(
		async (error: Error) => console.error("Storage error:", error),
		ephemeralStorageProvider,
		async () => null,
	)

	const cacheManagement = lazyMemoized(async () => {
		const { CacheManagementFacade } = await import("../tutanota/src/applications/common/api/worker/facades/lazy/CacheManagementFacade.js")
		return new CacheManagementFacade(base.user, base.cachingEntityClient, base.cache as any)
	})

	const lastProcessedEventBatchStorageFacade = lazyMemoized(async () => new NoOpLastProcessedEventBatchStorageFacade())

	const browserData: BrowserData = {
		needsMicrotaskHack: false,
		needsExplicitIDBIds: false,
		indexedDbSupported: true,
		clientPlatform: ClientPlatform.UNKNOWN,
	}

	console.log("Creating BaseLocator...")
	const clientModelInfo = initClientModels(apps)
	const nativeCryptoFacade: Nullable<NativeCryptoFacade> = null
	base = await createBaseLocator({
		worker,
		clientModelInfo,
		browserData,
		loginListenerProvider: () => noOpLoginListener,
		maybeUninitializedStorage,
		lastProcessedEventBatchStorageFacade,
		cacheManagement,
		identityKeyTrustDatabase,
		domainConfig: new DomainConfigProvider().getCurrentDomainConfig(),
		argon2idFacade,
		rsa: new RsaWeb(),
		fileFacade: { writeToAppDir: async () => {}, readFromAppDir: async () => new Uint8Array(), deleteFromAppDir: async () => {} },
		nativeCryptoFacade,
		entityMigratorFactory: ({
			cryptoWrapper,
			user,
			keyLoader,
			cachingEntityClient,
			serviceExecutor,
			typeModelResolver,
			instancePipeline,
			restClient,
			crypto,
		}) =>
			new TutanotaEntityMigrator(
				cryptoWrapper,
				user,
				keyLoader,
				cachingEntityClient,
				serviceExecutor,
				typeModelResolver,
				instancePipeline,
				restClient,
				crypto,
			),
		entityRestCache: (entityRestClient, patchMerger, typeModelResolver, lastProcessed) =>
			new DefaultEntityRestCache(entityRestClient, maybeUninitializedStorage, typeModelResolver, patchMerger, lastProcessed),
	})
	await base.login.createSession(loginAddress, passphrase, "Linux node", SessionType.Temporary, null)

	return base
}

async function getMailFacade(base: BaseLocator) {
	const blobFacade = {} as BlobFacade
	const desktopFileFacade = {} as DesktopFileFacade
	const exportFacade = {} as ExportFacade
	const fileApp = new NativeFileApp(desktopFileFacade, exportFacade)
	const entityClient = new EntityClient(base.entityRestClient, base.typeModelResolver)
	return new MailFacade(
		base.user,
		entityClient,
		base.crypto,
		base.cryptoWrapper,
		base.serviceExecutor,
		blobFacade,
		fileApp,
		base.login,
		base.keyLoader,
		base.publicEncryptionKeyProvider,
	)
}

function startServer(onMessage: (body: string) => Promise<SendMailReturn>) {
	if (fs.existsSync(SOCKET_PATH)) {
		fs.unlinkSync(SOCKET_PATH)
	}

	const server = http.createServer((req, res) => {
		if (req.method === "POST") {
			let body = ""
			req.on("data", (chunk) => {
				body += chunk.toString()
			})
			req.on("end", async () => {
				let resp: SendMailReturn = await onMessage(body) // hand off the message to your handler
				res.writeHead(200, { "Content-Type": "application/json" })
				res.end(JSON.stringify(resp))
			})
		} else {
			res.writeHead(200, { "Content-Type": "text/plain" })
			res.end("Server is running\n")
		}
	})

	server.listen(SOCKET_PATH, () => {
		console.log(`Server listening on unix socket: ${SOCKET_PATH}`)
		fs.chmodSync(SOCKET_PATH, "766")
	})

	process.on("SIGINT", () => {
		server.close(() => {
			fs.unlinkSync(SOCKET_PATH)
			process.exit(0)
		})
	})

	return server
}

type SendMailReturn = {
	messageId: string
	recipient: string
}

async function getRecipientType(baseLocator: BaseLocator, recipientAddress: string): Promise<RecipientType> {
	const requestData = createPublicKeyGetIn({
		version: null,
		identifier: recipientAddress,
		identifierType: PublicKeyIdentifierType.MAIL_ADDRESS,
	})
	try {
		await baseLocator.serviceExecutor.execute(PublicKeyService_GET, requestData, null)
	} catch (e) {
		if (e instanceof NotFoundError) {
			console.info("Recipient is external")
			return RecipientType.EXTERNAL
		} else if (e instanceof BadRequestError) {
			console.error(e)
			throw e
		} else {
			console.error("Request to PublicKeyService was not Ok")
			console.error(e)
			throw e
		}
	}

	return RecipientType.INTERNAL
}

async function sendMail(baseLocator: BaseLocator, mailFacade: MailFacade, msg: SocketMessage, config: Record<string, string>): Promise<SendMailReturn> {
	//TODO handle multiple recipients
	const recipientType = await getRecipientType(baseLocator, msg.recipients[0].address)
	console.log("Sending an email to a recipient of type: " + recipientType)
	const r: PartialRecipient = {
		address: msg.recipients[0].address,
		name: msg.recipients[0].name,
		type: recipientType,
	}
	const d = {
		subject: msg.subject,
		bodyText: msg.body,
		senderMailAddress: config.LOGIN_ADDRESS,
		senderName: msg.sender.name ?? "",
		toRecipients: [r],
		ccRecipients: [],
		bccRecipients: [],
		conversationType: ConversationType.NEW,
		previousMessageId: null,
		attachments: null,
		confidential: recipientType === RecipientType.INTERNAL,
		replyTos: [],
		method: MailMethod.NONE,
	}
	const newMail = await mailFacade.createDraft(d)
	const recipient: Recipient = {
		address: r.address,
		name: r.name,
		type: r.type,
		contact: null,
		verificationState: PresentableKeyVerificationState.NONE,
	}
	let sendDraftReturn = await mailFacade.sendDraft(newMail, [recipient], "en", null, false)
	return { messageId: sendDraftReturn.messageId, recipient: recipient.address }
}
function readConfig(filePath: string) {
	const config: Record<string, string> = {}
	const lines = fs.readFileSync(filePath, "utf-8").split("\n")

	for (let line of lines) {
		line = line.trim()

		// skip blank lines and full-line comments
		if (!line || line.startsWith("#")) continue

		const eqIndex = line.indexOf("=")
		if (eqIndex === -1) continue // skip malformed lines

		const key = line.slice(0, eqIndex).trim()
		const value = line.slice(eqIndex + 1).trim()

		config[key] = value
	}

	return config
}
async function run() {
	const config = readConfig(CONFIG_PATH)
	const baseLocator = await getBaseLocator(config.LOGIN_ADDRESS, config.PASSWORD)
	const mailFacade = await getMailFacade(baseLocator)
	console.log("Created Mail Facade")

	startServer(async (newMessage: string) => {
		// fire-and-forget async handling
		const msg: SocketMessage = JSON.parse(newMessage)
		return await sendMail(baseLocator, mailFacade, msg, config)
	})
}

await run()
