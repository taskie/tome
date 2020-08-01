import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
  groupName: string;
  path: string;
};

export const StatLink: React.FC<Props> = ({ workspaceName, groupName, path }) => (
  <Link
    href={
      uri`/${workspaceName}/stats/${groupName}/` +
      path
        .split("/")
        .map((s) => encodeURIComponent(s))
        .join("/")
    }
  >
    <a>{path}</a>
  </Link>
);

export default StatLink;
